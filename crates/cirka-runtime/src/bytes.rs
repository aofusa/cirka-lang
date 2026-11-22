//! Byte string operations of the support library (FEAT-0095): copying,
//! searching for a byte and comparing ranges of BYTES objects with the C
//! library's `memmove`, `memchr` and `memcmp` (vectorized), instead of
//! loops over one byte at a time. Ranges out of bounds panic, as the
//! checked byte accesses they replace do.

use crate::object::{Obj, word};
use crate::start::panic_msg;

use core::ffi::c_void;

unsafe extern "C" {
    fn memchr(s: *const c_void, c: i32, n: usize) -> *const c_void;
    fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> i32;
}

/// The length of a BYTES object.
///
/// # Safety
/// `o` must be a live BYTES object.
#[inline(always)]
unsafe fn len(o: Obj) -> i64 {
    // SAFETY: word 0 of a BYTES object is its length.
    unsafe { *word(o, 0) as i64 }
}

/// The address of byte `i` of a BYTES object.
///
/// # Safety
/// `o` must be a live BYTES object; `i` at most its length.
#[inline(always)]
unsafe fn at(o: Obj, i: i64) -> *mut u8 {
    // SAFETY: the data of a BYTES object follows its length word.
    unsafe { (word(o, 1) as *mut u8).add(i as usize) }
}

/// Whether `[i, i + n)` (with `n >= 0`) lies in a BYTES object of length
/// `len`.
#[inline(always)]
fn fits(i: i64, n: i64, len: i64) -> bool {
    i >= 0 && n <= len - i
}

#[cold]
fn out_of_bounds() -> ! {
    panic_msg(b"index out of bounds")
}

/// Copies `src[i..i + n]` to `dst[off..off + n]` (`cirka$bytes_copy_at`;
/// the two may be the same object). Nothing when `n <= 0`. Returns 0.
///
/// # Safety
/// `dst` and `src` must be live BYTES objects, and nothing else may read
/// `dst` meanwhile (it is the unique buffer being written).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_copy(dst: Obj, off: i64, src: Obj, i: i64, n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    // SAFETY: both are live BYTES objects (above); the ranges are checked
    // to lie in them, and `copy` allows them to overlap.
    unsafe {
        if !fits(off, n, len(dst)) || !fits(i, n, len(src)) {
            out_of_bounds();
        }
        core::ptr::copy(at(src, i), at(dst, off), n as usize);
    }
    0
}

/// The first index in `[i, j)` of the byte `b` in `s`, or -1 (also when
/// `b` is not a byte value; `cirka$bytes_find_byte_loop`).
///
/// # Safety
/// `s` must be a live BYTES object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_index(s: Obj, b: i64, i: i64, j: i64) -> i64 {
    if i >= j {
        return -1;
    }
    // SAFETY: `s` is a live BYTES object (above) and `[i, j)` is checked to
    // lie in it.
    unsafe {
        if !fits(i, j - i, len(s)) {
            out_of_bounds();
        }
        // No byte equals a value outside 0..=255.
        if !(0..=255).contains(&b) {
            return -1;
        }
        let base = at(s, 0) as *const u8;
        let p = memchr(
            base.add(i as usize).cast(),
            b as u8 as i32,
            (j - i) as usize,
        );
        if p.is_null() {
            -1
        } else {
            p.cast::<u8>().offset_from(base) as i64
        }
    }
}

/// Whether `s[i..i + n] == t[j..j + n]`: 1 or 0, and 1 when `n <= 0`
/// (`cirka$bytes_range_eq_loop`).
///
/// # Safety
/// `s` and `t` must be live BYTES objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_eq(s: Obj, i: i64, t: Obj, j: i64, n: i64) -> i64 {
    if n <= 0 {
        return 1;
    }
    // SAFETY: both are live BYTES objects (above); the ranges are checked
    // to lie in them.
    unsafe {
        if !fits(i, n, len(s)) || !fits(j, n, len(t)) {
            out_of_bounds();
        }
        (memcmp(at(s, i).cast(), at(t, j).cast(), n as usize) == 0) as i64
    }
}
