//! Externs of Haskell's `Integer` (FEAT-0114, 05-runtime.md §5) over
//! [`cirka_bigint`]. Big integers are BYTES objects in the encoded form of
//! `cirka_bigint` (sign byte, little-endian 32-bit limbs); values that fit
//! in an `Int` are kept small by the library (`IS`), so these are only
//! called for large values or on overflow. Every function allocates exactly
//! one object, its result (scratch memory comes from `malloc`).

use crate::io::{bytes_of, fatal};
use crate::object::Obj;
use crate::sys;
use cirka_bigint::{self as big, Int};
use core::ffi::c_void;

/// A malloc'ed buffer of limbs, freed on drop.
struct Limbs {
    p: *mut u32,
    n: usize,
}

impl Limbs {
    fn new(n: usize) -> Limbs {
        // SAFETY: plain allocation; at least one limb so the pointer is
        // never null for a successful call.
        let p = unsafe { sys::malloc(4 * n.max(1)) } as *mut u32;
        if p.is_null() {
            fatal(b"out of memory", 251);
        }
        // SAFETY: `p` holds `n` limbs.
        unsafe { core::ptr::write_bytes(p, 0, n.max(1)) };
        Limbs { p, n }
    }

    fn slice(&mut self) -> &mut [u32] {
        // SAFETY: `p` holds `n` initialized limbs owned by `self`.
        unsafe { core::slice::from_raw_parts_mut(self.p, self.n) }
    }
}

impl Drop for Limbs {
    fn drop(&mut self) {
        // SAFETY: allocated by `malloc` in `new`.
        unsafe { sys::free(self.p as *mut c_void) }
    }
}

/// The integer in BYTES object `o`, decoded into a fresh buffer.
///
/// # Safety
/// `o` must be a live BYTES object.
unsafe fn load(o: Obj) -> (bool, Limbs, usize) {
    // SAFETY: guaranteed by the caller.
    let b = unsafe { bytes_of(o) };
    let mut l = Limbs::new(big::decoded_len(b));
    let (neg, n) = big::decode(b, l.slice());
    (neg, l, n)
}

/// A new BYTES object holding `neg`/`mag`.
fn store(neg: bool, mag: &[u32]) -> Obj {
    let len = big::encoded_len(mag.len());
    // SAFETY: plain allocation of `len` bytes.
    let buf = unsafe { sys::malloc(len) } as *mut u8;
    if buf.is_null() {
        fatal(b"out of memory", 251);
    }
    // SAFETY: `buf` holds `len` bytes; the BYTES object copies them.
    unsafe {
        let s = core::slice::from_raw_parts_mut(buf, len);
        let n = big::encode(neg, mag, s);
        let o = crate::cirka_bytes_from(buf, n);
        sys::free(buf as *mut c_void);
        o
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn cirka_big_from_int(x: i64) -> Obj {
    let mut l = [0u32; 2];
    let (neg, n) = big::from_i64(x, &mut l);
    store(neg, &l[..n])
}

/// 1 when the integer fits in an `Int`.
///
/// # Safety
/// `a` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_big_fits_int(a: Obj) -> i64 {
    // SAFETY: guaranteed by the caller.
    let (neg, mut l, n) = unsafe { load(a) };
    i64::from(big::fits_i64(Int {
        neg,
        mag: &l.slice()[..n],
    }))
}

/// The low 64 bits (two's complement).
///
/// # Safety
/// `a` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_big_to_int(a: Obj) -> i64 {
    // SAFETY: guaranteed by the caller.
    let (neg, mut l, n) = unsafe { load(a) };
    big::to_i64_wrapping(Int {
        neg,
        mag: &l.slice()[..n],
    })
}

/// # Safety
/// `a` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_big_to_double(a: Obj) -> f64 {
    // SAFETY: guaranteed by the caller.
    let (neg, mut l, n) = unsafe { load(a) };
    big::to_f64(Int {
        neg,
        mag: &l.slice()[..n],
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn cirka_big_from_double(x: f64) -> Obj {
    let mut l = [0u32; big::F64_LIMBS];
    let (neg, n) = big::from_f64(x, &mut l);
    store(neg, &l[..n])
}

/// The operation `op` of two integers: 0 add, 1 sub, 2 mul, 3 quot, 4 rem
/// (the divisor is not zero: the library checks).
///
/// # Safety
/// `a` and `b` must be live BYTES objects (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_big_op(op: i64, a: Obj, b: Obj) -> Obj {
    // SAFETY: guaranteed by the caller.
    let (an, mut al, alen) = unsafe { load(a) };
    // SAFETY: guaranteed by the caller.
    let (bn, mut bl, blen) = unsafe { load(b) };
    let x = Int {
        neg: an,
        mag: &al.slice()[..alen],
    };
    // Copy b's limbs out so both views can coexist with `out`.
    let mut bcopy = Limbs::new(blen);
    bcopy.slice().copy_from_slice(&bl.slice()[..blen]);
    let y = Int {
        neg: bn,
        mag: &bcopy.slice()[..blen],
    };
    match op {
        0 | 1 => {
            let mut out = Limbs::new(big::add_len(alen, blen));
            let (neg, n) = if op == 0 {
                big::add(x, y, out.slice())
            } else {
                big::sub(x, y, out.slice())
            };
            store(neg, &out.slice()[..n])
        }
        2 => {
            let mut out = Limbs::new(alen + blen);
            let (neg, n) = big::mul(x, y, out.slice());
            store(neg, &out.slice()[..n])
        }
        _ => {
            let mut q = Limbs::new(alen + 1);
            let mut r = Limbs::new(blen.max(1));
            let mut s = Limbs::new(big::divrem_scratch_len(alen, blen));
            let ((qn, ql), (rn, rl)) = big::quot_rem(x, y, q.slice(), r.slice(), s.slice());
            if op == 3 {
                store(qn, &q.slice()[..ql])
            } else {
                store(rn, &r.slice()[..rl])
            }
        }
    }
}

/// -1, 0 or 1.
///
/// # Safety
/// `a` and `b` must be live BYTES objects (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_big_cmp(a: Obj, b: Obj) -> i64 {
    // SAFETY: guaranteed by the caller.
    let (an, mut al, alen) = unsafe { load(a) };
    // SAFETY: guaranteed by the caller.
    let (bn, mut bl, blen) = unsafe { load(b) };
    let x = Int {
        neg: an,
        mag: &al.slice()[..alen],
    };
    let y = Int {
        neg: bn,
        mag: &bl.slice()[..blen],
    };
    big::cmp(x, y) as i64
}

/// The decimal digits (with `-`) as a BYTES object.
///
/// # Safety
/// `a` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_big_show(a: Obj) -> Obj {
    // SAFETY: guaranteed by the caller.
    let (neg, mut l, n) = unsafe { load(a) };
    let mut tmp = Limbs::new(n);
    let len = big::decimal_len(n);
    // SAFETY: plain allocation of `len` bytes.
    let buf = unsafe { sys::malloc(len) } as *mut u8;
    if buf.is_null() {
        fatal(b"out of memory", 251);
    }
    // SAFETY: `buf` holds `len` bytes.
    unsafe {
        let out = core::slice::from_raw_parts_mut(buf, len);
        let k = big::to_decimal(
            Int {
                neg,
                mag: &l.slice()[..n],
            },
            tmp.slice(),
            out,
        );
        let o = crate::cirka_bytes_from(buf, k);
        sys::free(buf as *mut c_void);
        o
    }
}

/// The integer with decimal `digits` (checked by the library), negated
/// when `neg != 0`.
///
/// # Safety
/// `digits` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_big_read(digits: Obj, neg: i64) -> Obj {
    // SAFETY: guaranteed by the caller.
    let d = unsafe { bytes_of(digits) };
    let mut l = Limbs::new(big::from_decimal_len(d.len()));
    let n = big::from_decimal(d, l.slice()).unwrap_or(0);
    store(neg != 0, &l.slice()[..n])
}
