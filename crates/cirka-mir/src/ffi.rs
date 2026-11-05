//! Foreign calls in the interpreter (x86_64 System V only).
//!
//! The interpreter cannot generate call stubs, so a foreign function is
//! called through one fixed function type with six integer and eight
//! floating point parameters: in the System V ABI the two register classes
//! are assigned independently and in order, extra register arguments are
//! ignored by the callee, and the result is in `rax` or `xmm0`. This covers
//! every non-variadic function whose integer (pointer) and floating point
//! arguments fit in registers, which is what the tests need; the native
//! backend declares the real signature.
//!
//! This module and the raw memory access below are the interpreter's FFI
//! boundary, the only `unsafe` code of this crate.

use crate::CTy;
use std::ffi::{CString, c_char, c_void};

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// `RTLD_DEFAULT` of glibc: search the global symbols of the process.
const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();

const MAX_INT_ARGS: usize = 6;
const MAX_FLOAT_ARGS: usize = 8;

type IntFn = unsafe extern "C" fn(
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
) -> i64;
type FloatFn = unsafe extern "C" fn(
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
) -> f64;

/// An argument of a foreign call: the interpreter's `i64` / `f64` value.
#[derive(Clone, Copy, Debug)]
pub enum Arg {
    I(i64),
    F(f64),
}

/// The result: an `i64` (integers, pointers, `void` as 0) or an `f64`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ret {
    I(i64),
    F(f64),
}

/// Calls the C function `name` with the C signature `params -> ret`.
pub fn call(name: &str, params: &[CTy], ret: CTy, args: &[Arg]) -> Result<Ret, String> {
    let sym = CString::new(name).map_err(|_| format!("bad foreign name `{name}`"))?;
    // SAFETY: `dlsym` only reads the NUL-terminated name; `RTLD_DEFAULT`
    // is a valid pseudo handle on glibc.
    let f = unsafe { dlsym(RTLD_DEFAULT, sym.as_ptr()) };
    if f.is_null() {
        return Err(format!(
            "foreign function `{name}` is not available in the interpreter process"
        ));
    }
    let mut ints = [0i64; MAX_INT_ARGS];
    let mut floats = [0f64; MAX_FLOAT_ARGS];
    let (mut ni, mut nf) = (0, 0);
    for (a, t) in args.iter().zip(params) {
        match (a, t) {
            (Arg::F(x), CTy::F32) | (Arg::F(x), CTy::F64) => {
                if nf == MAX_FLOAT_ARGS {
                    return Err(format!("too many floating point arguments for `{name}`"));
                }
                // A `float` argument is the low 32 bits of the register.
                floats[nf] = if *t == CTy::F32 {
                    f64::from_bits(u64::from((*x as f32).to_bits()))
                } else {
                    *x
                };
                nf += 1;
            }
            (Arg::I(x), t) if !t.is_float() && *t != CTy::Void => {
                if ni == MAX_INT_ARGS {
                    return Err(format!("too many integer arguments for `{name}`"));
                }
                ints[ni] = narrow(*x, *t);
                ni += 1;
            }
            _ => return Err(format!("argument of `{name}` does not match its C type")),
        }
    }
    let [i0, i1, i2, i3, i4, i5] = ints;
    let [f0, f1, f2, f3, f4, f5, f6, f7] = floats;
    if ret.is_float() {
        // SAFETY: the symbol is a function (declared by the program's
        // foreign import, whose correctness is the programmer's
        // responsibility as in C); see the module comment for why the
        // fixed register signature is call compatible.
        let g: FloatFn = unsafe { std::mem::transmute::<*mut c_void, FloatFn>(f) };
        // SAFETY: as above.
        let r = unsafe { g(i0, i1, i2, i3, i4, i5, f0, f1, f2, f3, f4, f5, f6, f7) };
        Ok(Ret::F(if ret == CTy::F32 {
            f64::from(f32::from_bits(r.to_bits() as u32))
        } else {
            r
        }))
    } else {
        // SAFETY: as above.
        let g: IntFn = unsafe { std::mem::transmute::<*mut c_void, IntFn>(f) };
        // SAFETY: as above.
        let r = unsafe { g(i0, i1, i2, i3, i4, i5, f0, f1, f2, f3, f4, f5, f6, f7) };
        Ok(Ret::I(match ret {
            CTy::Void => 0,
            t => narrow(r, t),
        }))
    }
}

/// An integer as the C type `t` holds it, extended back to 64 bits.
pub fn narrow(x: i64, t: CTy) -> i64 {
    match t {
        CTy::I8 => x as i8 as i64,
        CTy::U8 => x as u8 as i64,
        CTy::I16 => x as i16 as i64,
        CTy::U16 => x as u16 as i64,
        CTy::I32 => x as i32 as i64,
        CTy::U32 => x as u32 as i64,
        _ => x,
    }
}

/// The C type read or written by `cirka_mem_*` (`kind` as in the runtime).
fn kind_ty(kind: i64) -> Option<(CTy, usize)> {
    Some(match kind {
        0 => (CTy::I8, 1),
        1 => (CTy::U8, 1),
        2 => (CTy::I16, 2),
        3 => (CTy::U16, 2),
        4 => (CTy::I32, 4),
        5 => (CTy::U32, 4),
        6 => (CTy::I64, 8),
        _ => return None,
    })
}

/// `cirka_mem_read`: an integer of the given kind at `addr`.
pub fn mem_read(addr: i64, kind: i64) -> Result<i64, String> {
    let (t, n) = kind_ty(kind).ok_or("bad memory access kind")?;
    check(addr)?;
    let mut b = [0u8; 8];
    // SAFETY: reading foreign memory through a `Ptr` is unchecked, as in C
    // and GHC (`peek`); the address is not null.
    unsafe { std::ptr::copy_nonoverlapping(addr as usize as *const u8, b.as_mut_ptr(), n) };
    Ok(narrow(i64::from_le_bytes(b), t))
}

/// `cirka_mem_write`: stores the low bytes of `val` at `addr`.
pub fn mem_write(addr: i64, kind: i64, val: i64) -> Result<(), String> {
    let (_, n) = kind_ty(kind).ok_or("bad memory access kind")?;
    check(addr)?;
    let b = val.to_le_bytes();
    // SAFETY: as in `mem_read` (`poke`).
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), addr as usize as *mut u8, n) };
    Ok(())
}

/// `cirka_mem_read_f`: a `float` (kind 0) or `double` (kind 1).
pub fn mem_read_f(addr: i64, kind: i64) -> Result<f64, String> {
    check(addr)?;
    Ok(if kind == 0 {
        f64::from(f32::from_bits(mem_read(addr, 5)? as u32))
    } else {
        f64::from_bits(mem_read(addr, 6)? as u64)
    })
}

/// `cirka_mem_write_f`: stores a `float` (kind 0) or `double` (kind 1).
pub fn mem_write_f(addr: i64, kind: i64, val: f64) -> Result<(), String> {
    if kind == 0 {
        mem_write(addr, 5, i64::from((val as f32).to_bits()))
    } else {
        mem_write(addr, 6, val.to_bits() as i64)
    }
}

fn check(addr: i64) -> Result<(), String> {
    if addr == 0 {
        Err("null pointer dereference".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_libc_and_libm() {
        let r = call("labs", &[CTy::I64], CTy::I64, &[Arg::I(-5)]).unwrap();
        assert_eq!(r, Ret::I(5));
        let r = call("cos", &[CTy::F64], CTy::F64, &[Arg::F(0.0)]).unwrap();
        assert_eq!(r, Ret::F(1.0));
        let r = call(
            "ldexpf",
            &[CTy::F32, CTy::I32],
            CTy::F32,
            &[Arg::F(1.5), Arg::I(3)],
        )
        .unwrap();
        assert_eq!(r, Ret::F(12.0));
        let r = call("abs", &[CTy::I32], CTy::I32, &[Arg::I(-7)]).unwrap();
        assert_eq!(r, Ret::I(7));
        assert!(call("no_such_function_xyz", &[], CTy::Void, &[]).is_err());
    }

    #[test]
    fn memory_round_trip() {
        let mut buf = [0u8; 16];
        let p = buf.as_mut_ptr() as i64;
        mem_write(p, 4, -2).unwrap();
        assert_eq!(mem_read(p, 4).unwrap(), -2);
        assert_eq!(mem_read(p, 5).unwrap(), 0xffff_fffe);
        assert_eq!(mem_read(p, 1).unwrap(), 0xfe);
        mem_write_f(p + 8, 0, 0.5).unwrap();
        assert_eq!(mem_read_f(p + 8, 0).unwrap(), 0.5);
        mem_write_f(p + 8, 1, -2.25).unwrap();
        assert_eq!(mem_read_f(p + 8, 1).unwrap(), -2.25);
        assert!(mem_read(0, 6).is_err());
    }
}
