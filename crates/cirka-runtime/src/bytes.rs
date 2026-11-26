//! Byte string operations of the support library (FEAT-0095): copying,
//! searching for a byte and comparing ranges of BYTES objects with the C
//! library's `memmove`, `memchr` and `memcmp` (vectorized), instead of
//! loops over one byte at a time. Ranges out of bounds are an error, as
//! for the checked byte accesses they replace (an exception when the
//! program unwinds them: the function returns, FEAT-0040). Also resizing
//! a BYTES object in place (FEAT-0096).

use crate::object::{Obj, word};

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

static OUT_OF_BOUNDS: crate::exception::StaticBytes<19> =
    crate::exception::StaticBytes::new(b"index out of bounds");

/// A range out of bounds: ends the program, or makes the error pending
/// (the caller then returns at once).
#[cold]
fn out_of_bounds() {
    crate::exception::fail_static(&OUT_OF_BOUNDS)
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
            return 0;
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
            return -1;
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
            return 0;
        }
        (memcmp(at(s, i).cast(), at(t, j).cast(), n as usize) == 0) as i64
    }
}

/// The size of the allocation of a BYTES object of `n` bytes (as
/// `cirka_bytes_new` allocates it; a BYTES object always has at least as
/// much memory as this for its length).
#[inline(always)]
pub(crate) fn bytes_size(n: usize) -> usize {
    16 + n.div_ceil(8) * 8
}

/// `b` (consumed) with `n` bytes: its first `min(len b, n)` bytes are
/// kept and, when `zero`, those added are 0 (otherwise they are left as
/// they are, for a caller that writes them at once). A unique object from
/// `malloc` is resized in place with `realloc` (FEAT-0096); a shrunk one
/// keeps its memory unless less than half of it is still used. Otherwise
/// the bytes go to a new object, and `b` is dropped. Resizing a unique
/// object counts as an allocation and a free whichever way it goes, so
/// that every runtime and the MIR interpreter count the same.
///
/// # Safety
/// `b` must be a live BYTES object owned by the caller.
pub(crate) unsafe fn resize(b: Obj, n: usize, zero: bool) -> Obj {
    // SAFETY: `b` is a live BYTES object of `old` bytes, owned by the
    // caller; when unique nobody else refers to it, so it may move.
    unsafe {
        let old = len(b) as usize;
        let keep = old.min(n);
        if crate::object::rc(b) == 1 {
            if n <= old {
                crate::alloc::stat_alloc();
                crate::alloc::stat_free();
                *word(b, 0) = n;
                if bytes_size(n) < bytes_size(old) / 2
                    && let Some(p) = crate::alloc::resize_system(b, bytes_size(n))
                {
                    return p;
                }
                return b;
            }
            if let Some(p) = crate::alloc::resize_system(b, bytes_size(n)) {
                crate::alloc::stat_alloc();
                crate::alloc::stat_free();
                *word(p, 0) = n;
                if zero {
                    core::ptr::write_bytes(at(p, old as i64), 0, n - old);
                }
                return p;
            }
        } else {
            // A copy because the string is shared (not because it grows).
            crate::alloc::stats().array_copies += 1;
        }
        let o = crate::cirka_bytes_new(n as i64);
        core::ptr::copy_nonoverlapping(at(b, 0), at(o, 0), keep);
        crate::object::cirka_drop(b);
        o
    }
}

/// `bytes_resize b n` (FEAT-0096): `b` (consumed) with `max(n, 0)` bytes,
/// its first bytes kept and those added 0; in place when `b` is unique
/// and the allocator can.
///
/// # Safety
/// `b` must be a live BYTES object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_resize(b: Obj, n: i64) -> Obj {
    // SAFETY: forwarded.
    unsafe { resize(b, n.max(0) as usize, true) }
}

/// The string of the `len` bytes at `buf + 16`, `buf` being a block of
/// `bytes_size(cap)` bytes from `malloc` (consumed): the block itself, its
/// header and length written, when it is large (it is then shrunk with
/// `realloc` if less than half of it is used); otherwise (and on the
/// checked runtime, whose objects it tracks) a copy, and the block is
/// freed. Counts one allocation (none for an empty string: it is the
/// static one of `cirka_bytes_from`, FEAT-1002).
///
/// # Safety
/// `buf` must come from `malloc` with `bytes_size(cap)` bytes, `len <= cap`.
pub(crate) unsafe fn adopt(buf: *mut u8, cap: usize, len: usize) -> Obj {
    // SAFETY: as documented; a block outside the region is freed with
    // `free` (`dealloc`), as every large object is.
    unsafe {
        if cfg!(feature = "debug-rt") || bytes_size(len) <= crate::alloc::MAX_SMALL {
            let o = crate::cirka_bytes_from(buf.add(16), len);
            crate::sys::free(buf as *mut c_void);
            return o;
        }
        let mut o = buf as Obj;
        if bytes_size(len) < bytes_size(cap) / 2 {
            let p = crate::sys::realloc(buf as *mut c_void, bytes_size(len));
            if !p.is_null() {
                o = p as Obj;
            }
        }
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 0;
        (*o).kind = crate::object::BYTES;
        *word(o, 0) = len;
        crate::alloc::stat_alloc();
        o
    }
}
