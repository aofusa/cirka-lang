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
