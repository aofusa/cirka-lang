//! Region arenas (04-memory-model.md §6, FEAT-0049).
//!
//! A region is a `FOREIGN` object (reference counted like any value) whose
//! pointer is the arena state and whose finalizer tears the arena down if
//! that has not happened yet. The arena hands out *cells*: 8-byte slots
//! bump-allocated in pages of `malloc`ed memory. A cell holds a counted
//! reference to a heap value; the cells are the region's list of
//! references to the heap, dropped all at once when the region ends.
//!
//! A cell is addressed by a plain integer (its address), which is not
//! reference counted: values may refer to cells of the same region in any
//! shape — cycles included — without forming reference count cycles, and
//! a stale address that is never dereferenced is harmless. The frontends'
//! types (rank-2 region variables) keep dereferences inside the region.

use crate::object::{self, FOREIGN, Obj, word};
use crate::{alloc, io, sys};
use core::ffi::c_void;

/// Cells per page (a page is two words of header and the cells: 4 KiB).
const CELLS: usize = 510;

#[repr(C)]
struct Page {
    next: *mut Page,
    used: usize,
    cells: [Obj; CELLS],
}

#[repr(C)]
struct Arena {
    /// The current page first.
    pages: *mut Page,
}

fn oom() -> ! {
    io::ewrite(b"cirka: out of memory in a region\n");
    // SAFETY: terminates the process.
    unsafe { sys::abort() }
}

/// Drops every cell of the arena and returns its pages. Idempotent; the
/// pages are detached first, so a finalizer run by a drop that reaches
/// this arena again finds it empty.
///
/// # Safety
/// `a` must be a live arena state.
unsafe fn teardown(a: *mut Arena) {
    // SAFETY: the pages and their used cells are valid; cells hold counted
    // references.
    unsafe {
        let mut p = (*a).pages;
        (*a).pages = core::ptr::null_mut();
        while !p.is_null() {
            for i in 0..(*p).used {
                object::cirka_drop((*p).cells[i]);
            }
            let next = (*p).next;
            sys::free(p as *mut c_void);
            p = next;
        }
    }
}

/// The finalizer of the region object.
extern "C" fn finalize(a: usize) {
    // SAFETY: `a` is the arena state of a region object being freed.
    unsafe {
        teardown(a as *mut Arena);
        sys::free(a as *mut c_void);
    }
}

/// A fresh region (an owned `FOREIGN` object).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_region_new() -> Obj {
    // SAFETY: fresh allocations; a FOREIGN object has its pointer in word 0
    // and its finalizer in word 1, and no pointer fields.
    unsafe {
        let a = sys::malloc(size_of::<Arena>()) as *mut Arena;
        if a.is_null() {
            oom();
        }
        (*a).pages = core::ptr::null_mut();
        let o = alloc::cirka_alloc(24) as Obj;
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 0;
        (*o).kind = FOREIGN;
        *word(o, 0) = a as usize;
        *word(o, 1) = finalize as extern "C" fn(usize) as usize;
        o
    }
}

/// Ends a region (borrowed): drops all its cells and returns its memory.
/// The object itself stays valid (and empty) until its count reaches zero.
///
/// # Safety
/// `r` must be a live region object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_region_free(r: Obj) -> i64 {
    // SAFETY: word 0 of a region object is its arena state.
    unsafe { teardown(*word(r, 0) as *mut Arena) };
    0
}

/// A new cell of region `r` (borrowed) holding `v` (borrowed; the cell
/// takes its own count). Returns the cell's address.
///
/// # Safety
/// `r` must be a live region object and `v` a live object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_region_cell(r: Obj, v: Obj) -> i64 {
    // SAFETY: as above; a page is full when `used == CELLS`.
    unsafe {
        let a = *word(r, 0) as *mut Arena;
        let mut p = (*a).pages;
        if p.is_null() || (*p).used == CELLS {
            let q = sys::malloc(size_of::<Page>()) as *mut Page;
            if q.is_null() {
                oom();
            }
            (*q).next = p;
            (*q).used = 0;
            (*a).pages = q;
            p = q;
        }
        object::cirka_dup(v);
        let i = (*p).used;
        (*p).cells[i] = v;
        (*p).used = i + 1;
        &raw mut (*p).cells[i] as i64
    }
}

/// The value of a cell (a new count for the caller).
///
/// # Safety
/// `c` must be the address of a cell of a live region.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_region_get(c: i64) -> Obj {
    // SAFETY: the cell holds a live object.
    unsafe {
        let v = *(c as *const Obj);
        object::cirka_dup(v);
        v
    }
}

/// Replaces the value of a cell by `v` (borrowed), releasing the old one.
///
/// # Safety
/// `c` must be the address of a cell of a live region, `v` a live object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_region_set(c: i64, v: Obj) -> i64 {
    // SAFETY: as above.
    unsafe {
        object::cirka_dup(v);
        let old = *(c as *const Obj);
        *(c as *mut Obj) = v;
        object::cirka_drop(old);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_LOCK as LOCK;
    use crate::object::CON;

    unsafe fn con(fields: &[usize], nptr: u8) -> Obj {
        // SAFETY: fresh allocation of a CON with `fields`.
        unsafe {
            let o = alloc::cirka_alloc(8 + 8 * fields.len()) as Obj;
            (*o).rc = 1;
            (*o).tag = 0;
            (*o).nptr = nptr;
            (*o).kind = CON;
            for (i, f) in fields.iter().enumerate() {
                *word(o, i) = *f;
            }
            o
        }
    }

    fn live() -> u64 {
        // SAFETY: the test lock is held.
        unsafe { alloc::STATS.get().live }
    }

    /// Values that refer to each other through cells (a ring of 2000
    /// nodes, more than a page) and to the region itself are all freed
    /// when the region ends.
    #[test]
    fn region_frees_cyclic_cells() {
        let _g = LOCK.lock().unwrap();
        let l0 = live();
        unsafe {
            let r = cirka_region_new();
            let n = 2000;
            let mut cells = Vec::new();
            for i in 0..n {
                let v = con(&[i], 0);
                cells.push(cirka_region_cell(r, v));
                object::cirka_drop(v);
            }
            // Node i = (region, cell i+1): a cycle through the cells.
            for i in 0..n {
                object::cirka_dup(r);
                let node = con(&[r as usize, cells[(i + 1) % n] as usize], 1);
                cirka_region_set(cells[i], node);
                object::cirka_drop(node);
            }
            let v = cirka_region_get(cells[5]);
            assert_eq!(*word(v, 1) as i64, cells[6]);
            object::cirka_drop(v);
            cirka_region_free(r);
            assert_eq!(live() - l0, 1, "only the region object is left");
            object::cirka_drop(r);
            assert_eq!(live(), l0);
        }
    }

    /// A region dropped without `cirka_region_free` is torn down by its
    /// finalizer.
    #[test]
    fn region_finalizer_tears_down() {
        let _g = LOCK.lock().unwrap();
        let l0 = live();
        unsafe {
            let r = cirka_region_new();
            let v = con(&[1], 0);
            cirka_region_cell(r, v);
            object::cirka_drop(v);
            object::cirka_drop(r);
            assert_eq!(live(), l0);
        }
    }
}
