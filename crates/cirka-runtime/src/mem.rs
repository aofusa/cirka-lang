//! Raw memory access for `Foreign.Storable` (`peek` / `poke`). Addresses
//! come from `Ptr` values; as in C and GHC they are not checked.
//!
//! `kind`: 0 `int8_t`, 1 `uint8_t`, 2 `int16_t`, 3 `uint16_t`, 4 `int32_t`,
//! 5 `uint32_t`, 6 `int64_t`; for the floating point functions 0 `float`
//! and 1 `double`. Integers are returned extended to 64 bits.

/// Reads an integer of the given kind at `addr`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_mem_read(addr: i64, kind: i64) -> i64 {
    let p = addr as usize;
    // SAFETY: `peek` on a `Ptr` is unchecked by contract (Foreign.Storable);
    // unaligned reads are allowed because C structures may be packed.
    unsafe {
        match kind {
            0 => i64::from((p as *const i8).read_unaligned()),
            1 => i64::from((p as *const u8).read_unaligned()),
            2 => i64::from((p as *const i16).read_unaligned()),
            3 => i64::from((p as *const u16).read_unaligned()),
            4 => i64::from((p as *const i32).read_unaligned()),
            5 => i64::from((p as *const u32).read_unaligned()),
            _ => (p as *const i64).read_unaligned(),
        }
    }
}

/// Stores the low bytes of `val` at `addr`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_mem_write(addr: i64, kind: i64, val: i64) {
    let p = addr as usize;
    // SAFETY: as in `cirka_mem_read` (`poke`).
    unsafe {
        match kind {
            0 | 1 => (p as *mut u8).write_unaligned(val as u8),
            2 | 3 => (p as *mut u16).write_unaligned(val as u16),
            4 | 5 => (p as *mut u32).write_unaligned(val as u32),
            _ => (p as *mut i64).write_unaligned(val),
        }
    }
}

/// Reads a `float` (kind 0) or `double` (kind 1) at `addr`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_mem_read_f(addr: i64, kind: i64) -> f64 {
    let p = addr as usize;
    // SAFETY: as in `cirka_mem_read`.
    unsafe {
        if kind == 0 {
            f64::from((p as *const f32).read_unaligned())
        } else {
            (p as *const f64).read_unaligned()
        }
    }
}

/// Stores a `float` (kind 0) or `double` (kind 1) at `addr`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_mem_write_f(addr: i64, kind: i64, val: f64) {
    let p = addr as usize;
    // SAFETY: as in `cirka_mem_read`.
    unsafe {
        if kind == 0 {
            (p as *mut f32).write_unaligned(val as f32)
        } else {
            (p as *mut f64).write_unaligned(val)
        }
    }
}

/// Copies the bytes of the BYTES object `b` (borrowed; an unboxed array or
/// a string) to C memory at `addr` (`Foreign.pokeUArray`). Returns 0.
///
/// # Safety
/// `b` must be a live BYTES object. That `addr` can be written for its
/// length is the program's responsibility, as with `poke`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_store(b: crate::object::Obj, addr: i64) -> i64 {
    // SAFETY: word 0 of a BYTES object is its length and its data follows
    // (offset 16); the destination is the program's (above).
    unsafe {
        let len = *crate::object::word(b, 0);
        if len > 0 {
            core::ptr::copy_nonoverlapping((b as *const u8).add(16), addr as usize as *mut u8, len);
        }
    }
    0
}

/// A new BYTES object holding the `len` bytes at C memory `addr`
/// (`Foreign.peekUArray`).
///
/// # Safety
/// That `addr` can be read for `len` bytes is the program's
/// responsibility, as with `peek`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_load(len: i64, addr: i64) -> crate::object::Obj {
    let o = crate::cirka_bytes_new(len);
    if len > 0 {
        // SAFETY: the fresh object has room for `len` bytes at offset 16;
        // the source is the program's (above).
        unsafe {
            core::ptr::copy_nonoverlapping(
                addr as usize as *const u8,
                (o as *mut u8).add(16),
                len as usize,
            );
        }
    }
    o
}

/// Overwrites the bytes of the BYTES object `b` (borrowed, not shared)
/// with as many bytes from C memory at `addr` (`Foreign.peekUArrayInto`,
/// after the array was made unique). Returns 0.
///
/// # Safety
/// `b` must be a live BYTES object that no other value shares. That
/// `addr` can be read for its length is the program's responsibility, as
/// with `peek`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_fill(b: crate::object::Obj, addr: i64) -> i64 {
    // SAFETY: as in `cirka_bytes_store`, in the other direction; nothing
    // else reads `b` (it is unique).
    unsafe {
        let len = *crate::object::word(b, 0);
        if len > 0 {
            core::ptr::copy_nonoverlapping(addr as usize as *const u8, (b as *mut u8).add(16), len);
        }
    }
    0
}

/// The address of the bytes of the BYTES object `b` (borrowed), for C to
/// use while `b` is alive (`Foreign.withUArrayPtr`). Objects never move.
///
/// # Safety
/// `b` must be a live BYTES object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_addr(b: crate::object::Obj) -> i64 {
    // SAFETY: the data of a BYTES object follows its length word.
    unsafe { crate::object::word(b, 1) as i64 }
}

/// Does nothing with the BYTES object `b` (borrowed): a use of `b` after C
/// used its address, so that `b` is not freed before (`withUArrayPtr`).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_bytes_touch(_b: crate::object::Obj) -> i64 {
    0
}
