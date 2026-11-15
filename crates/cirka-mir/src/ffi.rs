//! Foreign calls in the interpreter (x86_64 System V only).
//!
//! The interpreter cannot generate call stubs, so it calls every foreign
//! function through one C-variadic function type with six integer and
//! eight floating point register arguments and sixteen stack words: the
//! System V ABI assigns the two register classes independently and in
//! order and puts the arguments that do not fit on the stack in order, a
//! callee ignores extra arguments, and a variadic callee learns from `al`
//! (8 here) how many vector registers to save. The result comes back in
//! rax/rdx and xmm0/xmm1, read through `#[repr(C)]` pairs. Aggregates are
//! split into eightbytes or copied to the stack words exactly as the
//! backend passes them (`cirka_core::ctype`, 08-ffi.md §7.2). Callbacks
//! are not available: C would have to re-enter the interpreter.
//!
//! This module and the raw memory access below are the interpreter's FFI
//! boundary, the only `unsafe` code of this crate.

use crate::CTy;
use cirka_core::{CSig, CTypes, Pass};
use std::ffi::{CStr, CString, c_char, c_int, c_void};

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn __errno_location() -> *mut c_int;
}

/// `RTLD_DEFAULT` of glibc: search the global symbols of the process.
const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();
const RTLD_NOW: c_int = 2;
const RTLD_GLOBAL: c_int = 0x100;

const MAX_INT_ARGS: usize = 6;
const MAX_FLOAT_ARGS: usize = 8;
const STACK_WORDS: usize = 16;

/// Results in rax/rdx, xmm0/xmm1, rax/xmm0 and xmm0/rax.
#[repr(C)]
#[derive(Clone, Copy)]
struct Ii(i64, i64);
#[repr(C)]
#[derive(Clone, Copy)]
struct Ff(f64, f64);
#[repr(C)]
#[derive(Clone, Copy)]
struct If(i64, f64);
#[repr(C)]
#[derive(Clone, Copy)]
struct Fi(f64, i64);

type VarFn<R> = unsafe extern "C" fn(i64, ...) -> R;

/// An argument of a foreign call: the interpreter's `i64` / `f64` value.
#[derive(Clone, Copy, Debug)]
pub enum Arg {
    I(i64),
    F(f64),
}

impl Arg {
    fn int(self) -> i64 {
        match self {
            Arg::I(i) => i,
            Arg::F(x) => x.to_bits() as i64,
        }
    }
}

/// The result: an `i64` (integers, pointers, `void` and an aggregate
/// written to memory as 0) or an `f64`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ret {
    I(i64),
    F(f64),
}

/// The address of the C symbol `name` in the process.
pub fn symbol(name: &str) -> Result<i64, String> {
    let sym = CString::new(name).map_err(|_| format!("bad foreign name `{name}`"))?;
    // SAFETY: `dlsym` only reads the NUL-terminated name; `RTLD_DEFAULT`
    // is a valid pseudo handle on glibc.
    let f = unsafe { dlsym(RTLD_DEFAULT, sym.as_ptr()) };
    if f.is_null() {
        return Err(format!(
            "foreign symbol `{name}` is not available in the interpreter process"
        ));
    }
    Ok(f as i64)
}

/// Loads the shared library at `path` with its symbols global, so that
/// [`symbol`] finds them (the C helpers of tests, `link` libraries).
pub fn load_library(path: &std::path::Path) -> Result<(), String> {
    let p = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("bad library path {}", path.display()))?;
    // SAFETY: `dlopen` reads the NUL-terminated path; loading runs the
    // library's constructors, which is what linking it would do.
    let h = unsafe { dlopen(p.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
    if h.is_null() {
        // SAFETY: `dlerror` returns a NUL-terminated message (or null)
        // owned by the C library, read before any other `dl*` call here.
        let m = unsafe { dlerror() };
        let msg = if m.is_null() {
            String::from("unknown error")
        } else {
            // SAFETY: as above.
            unsafe { CStr::from_ptr(m) }.to_string_lossy().into_owned()
        };
        return Err(format!("cannot load {}: {msg}", path.display()));
    }
    Ok(())
}

/// The C library's `errno` of this thread.
pub fn errno() -> i64 {
    // SAFETY: glibc's `__errno_location` returns the address of this
    // thread's `errno`, valid for the thread's lifetime.
    i64::from(unsafe { *__errno_location() })
}

/// Sets the C library's `errno` of this thread.
pub fn set_errno(e: i64) {
    // SAFETY: as in `errno`.
    unsafe { *__errno_location() = e as c_int };
}

/// Calls the C function `name` with the C signature `sig`.
pub fn call(name: &str, ct: CTypes<'_>, sig: &CSig, args: &[Arg]) -> Result<Ret, String> {
    call_at(symbol(name)?, name, ct, sig, args)
}

/// Calls the C function at address `f` with the C signature `sig`. The
/// arguments are the Core values: an aggregate is the address of its
/// bytes, and an aggregate result is written to the address before them.
pub fn call_at(
    f: i64,
    name: &str,
    ct: CTypes<'_>,
    sig: &CSig,
    args: &[Arg],
) -> Result<Ret, String> {
    if f == 0 {
        return Err(format!("call of a null function pointer (`{name}`)"));
    }
    let (passes, ret) = ct
        .assign(&sig.params, &sig.ret)
        .ok_or_else(|| format!("unknown C type in the signature of `{name}`"))?;
    let agg_ret = sig.ret.is_agg();
    if args.len() != sig.params.len() + usize::from(agg_ret) {
        return Err(format!("wrong number of arguments for `{name}`"));
    }
    let out = if agg_ret { args[0].int() } else { 0 };
    let mut ints = [0i64; MAX_INT_ARGS];
    let mut floats = [0f64; MAX_FLOAT_ARGS];
    let mut stack = [0i64; STACK_WORDS];
    let (mut ni, mut nf, mut ns) = (0, 0, 0);
    let mut push_stack = |w: i64| {
        if ns == STACK_WORDS {
            return Err(format!("too many arguments on the stack for `{name}`"));
        }
        stack[ns] = w;
        ns += 1;
        Ok(())
    };
    if ret == Pass::Memory {
        check(out)?;
        ints[0] = out;
        ni = 1;
    }
    for ((pass, ty), a) in passes
        .iter()
        .zip(&sig.params)
        .zip(&args[usize::from(agg_ret)..])
    {
        match pass {
            Pass::Scalar(t) if t.is_float() => {
                let x = match a {
                    Arg::F(x) => *x,
                    Arg::I(_) => {
                        return Err(format!("argument of `{name}` does not match its C type"));
                    }
                };
                // A `float` argument is the low 32 bits of the register.
                let x = if *t == CTy::F32 {
                    f64::from_bits(u64::from((x as f32).to_bits()))
                } else {
                    x
                };
                if nf < MAX_FLOAT_ARGS {
                    floats[nf] = x;
                    nf += 1;
                } else {
                    push_stack(x.to_bits() as i64)?;
                }
            }
            Pass::Scalar(t) => {
                let Arg::I(x) = a else {
                    return Err(format!("argument of `{name}` does not match its C type"));
                };
                let x = narrow(*x, *t);
                if ni < MAX_INT_ARGS {
                    ints[ni] = x;
                    ni += 1;
                } else {
                    push_stack(x)?;
                }
            }
            Pass::Regs(pieces) => {
                let addr = a.int();
                check(addr)?;
                for (k, p) in pieces.iter().enumerate() {
                    let w = read_word(addr + 8 * k as i64, p.bytes() as usize);
                    if p.is_sse() {
                        floats[nf] = f64::from_bits(w as u64);
                        nf += 1;
                    } else {
                        ints[ni] = w;
                        ni += 1;
                    }
                }
            }
            Pass::Memory => {
                let addr = a.int();
                check(addr)?;
                let (size, _) = ct.size_align(ty).ok_or("unknown C type")?;
                for k in 0..size.div_ceil(8) {
                    let n = (size - 8 * k).min(8) as usize;
                    push_stack(read_word(addr + 8 * k as i64, n))?;
                }
            }
        }
    }
    let [i0, i1, i2, i3, i4, i5] = ints;
    let [f0, f1, f2, f3, f4, f5, f6, f7] = floats;
    let [
        s0,
        s1,
        s2,
        s3,
        s4,
        s5,
        s6,
        s7,
        s8,
        s9,
        s10,
        s11,
        s12,
        s13,
        s14,
        s15,
    ] = stack;
    macro_rules! go {
        ($r:ty) => {{
            // SAFETY: `f` is a function (declared by the program's foreign
            // import, whose correctness is the programmer's responsibility
            // as in C); see the module comment for why the register and
            // stack layout is call compatible with its real signature.
            let g: VarFn<$r> =
                unsafe { std::mem::transmute::<*mut c_void, VarFn<$r>>(f as *mut c_void) };
            // SAFETY: as above.
            unsafe {
                g(
                    i0, i1, i2, i3, i4, i5, f0, f1, f2, f3, f4, f5, f6, f7, s0, s1, s2, s3, s4, s5,
                    s6, s7, s8, s9, s10, s11, s12, s13, s14, s15,
                )
            }
        }};
    }
    match &ret {
        Pass::Scalar(CTy::Void) | Pass::Memory => {
            go!(Ii);
            Ok(Ret::I(0))
        }
        Pass::Scalar(t) if t.is_float() => {
            let r = go!(Ff).0;
            Ok(Ret::F(if *t == CTy::F32 {
                f64::from(f32::from_bits(r.to_bits() as u32))
            } else {
                r
            }))
        }
        Pass::Scalar(t) => Ok(Ret::I(narrow(go!(Ii).0, *t))),
        Pass::Regs(pieces) => {
            check(out)?;
            let words: [i64; 2] = match pieces.iter().map(|p| p.is_sse()).collect::<Vec<_>>()[..] {
                [false] | [false, false] => {
                    let r = go!(Ii);
                    [r.0, r.1]
                }
                [true] | [true, true] => {
                    let r = go!(Ff);
                    [r.0.to_bits() as i64, r.1.to_bits() as i64]
                }
                [false, true] => {
                    let r = go!(If);
                    [r.0, r.1.to_bits() as i64]
                }
                [true, false] => {
                    let r = go!(Fi);
                    [r.0.to_bits() as i64, r.1]
                }
                _ => return Err(format!("bad result of `{name}`")),
            };
            for (k, p) in pieces.iter().enumerate() {
                write_word(out + 8 * k as i64, p.bytes() as usize, words[k]);
            }
            Ok(Ret::I(0))
        }
    }
}

/// The first `n` (at most 8) bytes at `addr`, zero-extended.
fn read_word(addr: i64, n: usize) -> i64 {
    let mut b = [0u8; 8];
    // SAFETY: passing an aggregate reads its bytes as C would; the address
    // is not null (checked by the caller) and its validity is the
    // program's responsibility, as with `peek`.
    unsafe { std::ptr::copy_nonoverlapping(addr as usize as *const u8, b.as_mut_ptr(), n) };
    i64::from_le_bytes(b)
}

/// Stores the low `n` bytes of `w` at `addr`.
fn write_word(addr: i64, n: usize, w: i64) {
    let b = w.to_le_bytes();
    // SAFETY: as in `read_word` (the result of the program's size).
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), addr as usize as *mut u8, n) };
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

    fn sc(ps: &[CTy], r: CTy) -> CSig {
        CSig::scalars(ps, r)
    }

    #[test]
    fn calls_libc_and_libm() {
        let ct = CTypes(&[]);
        let r = call("labs", ct, &sc(&[CTy::I64], CTy::I64), &[Arg::I(-5)]).unwrap();
        assert_eq!(r, Ret::I(5));
        let r = call("cos", ct, &sc(&[CTy::F64], CTy::F64), &[Arg::F(0.0)]).unwrap();
        assert_eq!(r, Ret::F(1.0));
        let r = call(
            "ldexpf",
            ct,
            &sc(&[CTy::F32, CTy::I32], CTy::F32),
            &[Arg::F(1.5), Arg::I(3)],
        )
        .unwrap();
        assert_eq!(r, Ret::F(12.0));
        let r = call("abs", ct, &sc(&[CTy::I32], CTy::I32), &[Arg::I(-7)]).unwrap();
        assert_eq!(r, Ret::I(7));
        assert!(call("no_such_function_xyz", ct, &sc(&[], CTy::Void), &[]).is_err());
        assert!(symbol("environ").unwrap() != 0);
    }

    #[test]
    fn variadic_and_stack_arguments() {
        let ct = CTypes(&[]);
        let mut buf = [0u8; 64];
        let fmt = b"%d %d %d %d %d %d %d %.2f %s\0";
        let s = b"end\0";
        let mut sig = sc(
            &[
                CTy::Ptr,
                CTy::U64,
                CTy::Ptr,
                CTy::I32,
                CTy::I32,
                CTy::I32,
                CTy::I32,
                CTy::I32,
                CTy::I32,
                CTy::I32,
                CTy::F64,
                CTy::Ptr,
            ],
            CTy::I32,
        );
        sig.variadic = Some(3);
        let mut args = vec![
            Arg::I(buf.as_mut_ptr() as i64),
            Arg::I(64),
            Arg::I(fmt.as_ptr() as i64),
        ];
        args.extend((1..=7).map(Arg::I));
        args.push(Arg::F(2.5));
        args.push(Arg::I(s.as_ptr() as i64));
        let r = call("snprintf", ct, &sig, &args).unwrap();
        let want = "1 2 3 4 5 6 7 2.50 end";
        assert_eq!(r, Ret::I(want.len() as i64));
        assert_eq!(&buf[..want.len()], want.as_bytes());
    }

    #[test]
    fn aggregates_by_value() {
        use cirka_core::{CAgg, CType};
        let aggs = vec![
            CAgg {
                name: "div".into(),
                union: false,
                packed: false,
                fields: vec![CType::Scalar(CTy::I32), CType::Scalar(CTy::I32)],
            },
            CAgg {
                name: "ldiv".into(),
                union: false,
                packed: false,
                fields: vec![CType::Scalar(CTy::I64), CType::Scalar(CTy::I64)],
            },
            CAgg {
                name: "complex".into(),
                union: false,
                packed: false,
                fields: vec![CType::Scalar(CTy::F64), CType::Scalar(CTy::F64)],
            },
        ];
        let ct = CTypes(&aggs);
        let agg = |n: &str| CType::Agg(n.into());
        // div_t div(int, int): the result in rax.
        let mut out = [0i32; 2];
        let sig = CSig {
            params: vec![CType::Scalar(CTy::I32), CType::Scalar(CTy::I32)],
            ret: agg("div"),
            variadic: None,
        };
        let r = call(
            "div",
            ct,
            &sig,
            &[Arg::I(out.as_mut_ptr() as i64), Arg::I(-7), Arg::I(2)],
        );
        assert_eq!(r, Ok(Ret::I(0)));
        assert_eq!(out, [-3, -1]);
        // ldiv_t ldiv(long, long): in rax and rdx.
        let mut out = [0i64; 2];
        let sig = CSig {
            params: vec![CType::Scalar(CTy::I64), CType::Scalar(CTy::I64)],
            ret: agg("ldiv"),
            variadic: None,
        };
        call(
            "ldiv",
            ct,
            &sig,
            &[Arg::I(out.as_mut_ptr() as i64), Arg::I(1 << 40), Arg::I(3)],
        )
        .unwrap();
        assert_eq!(out, [(1 << 40) / 3, (1 << 40) % 3]);
        // double cabs(double complex): an argument in two SSE registers;
        // double complex csqrt(double complex): a result in xmm0 and xmm1.
        let z = [3.0f64, 4.0];
        let sig = CSig {
            params: vec![agg("complex")],
            ret: CType::Scalar(CTy::F64),
            variadic: None,
        };
        let r = call("cabs", ct, &sig, &[Arg::I(z.as_ptr() as i64)]).unwrap();
        assert_eq!(r, Ret::F(5.0));
        let z = [-4.0f64, 0.0];
        let mut out = [0f64; 2];
        let sig = CSig {
            params: vec![agg("complex")],
            ret: agg("complex"),
            variadic: None,
        };
        call(
            "csqrt",
            ct,
            &sig,
            &[Arg::I(out.as_mut_ptr() as i64), Arg::I(z.as_ptr() as i64)],
        )
        .unwrap();
        assert_eq!(out, [0.0, 2.0]);
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
