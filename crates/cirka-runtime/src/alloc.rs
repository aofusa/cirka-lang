//! Allocators (05-runtime.md §3).
//!
//! - `CIRKA_ALLOC=cirka` (default): size classes of 8..=256 bytes served
//!   from free lists. Pages of 64 KiB are carved out of one large reserved
//!   region, and a per-page table records each page's size class so that
//!   `dealloc` can find the class of any object from its address alone.
//!   Larger objects use `malloc`.
//! - `CIRKA_ALLOC=system`: `malloc`/`free` for everything (valgrind/ASan).
//!
//! The debug runtime (feature `debug-rt`) always uses `malloc` with a
//! prefix (see `debug.rs`).

#![cfg_attr(feature = "debug-rt", allow(dead_code, unused_imports))]

use crate::Global;
use crate::object::Obj;
use crate::sys;
use core::ffi::c_void;

const CLASSES: usize = 32;
const MAX_SMALL: usize = 8 * CLASSES;
const PAGE_SHIFT: usize = 16;
const PAGE: usize = 1 << PAGE_SHIFT;
const REGION: usize = 64 << 30;
const PAGES: usize = REGION / PAGE;

/// Free lists per size class (class i serves 8*(i+1) bytes). Exported so
/// that generated code may inline the fast path.
#[unsafe(no_mangle)]
pub static cirka_free_lists: Global<[*mut u8; CLASSES]> =
    Global::new([core::ptr::null_mut(); CLASSES]);

struct Region {
    base: usize,
    next_page: usize,
    /// 0 = not initialized, 1 = cirka allocator, 2 = system allocator.
    mode: u8,
}

static REGION_STATE: Global<Region> = Global::new(Region {
    base: 0,
    next_page: 0,
    mode: 0,
});
static PAGE_CLASS: Global<[u8; PAGES]> = Global::new([0; PAGES]);

/// Allocation statistics (printed with `CIRKA_STATS=1`).
pub struct Stats {
    pub allocs: u64,
    pub frees: u64,
    pub live: u64,
    pub max_live: u64,
}

pub static STATS: Global<Stats> = Global::new(Stats {
    allocs: 0,
    frees: 0,
    live: 0,
    max_live: 0,
});

#[inline(always)]
fn stat_alloc() {
    // SAFETY: allocation is single-threaded until FEAT-0045.
    let s = unsafe { STATS.get() };
    s.allocs += 1;
    s.live += 1;
    if s.live > s.max_live {
        s.max_live = s.live;
    }
}

#[inline(always)]
pub fn stat_free() {
    // SAFETY: see stat_alloc.
    let s = unsafe { STATS.get() };
    s.frees += 1;
    s.live = s.live.wrapping_sub(1);
}

#[cfg_attr(feature = "debug-rt", allow(dead_code))]
fn init() -> u8 {
    // SAFETY: called on the main thread before other threads exist.
    let r = unsafe { REGION_STATE.get() };
    if r.mode != 0 {
        return r.mode;
    }
    let system = matches!(sys::env(c"CIRKA_ALLOC"), Some(b"system"));
    if system || cfg!(feature = "debug-rt") {
        r.mode = 2;
        return 2;
    }
    // SAFETY: anonymous private mapping; MAP_NORESERVE only reserves
    // address space, physical pages are committed on first touch.
    let p = unsafe {
        sys::mmap(
            core::ptr::null_mut(),
            REGION,
            sys::PROT_READ | sys::PROT_WRITE,
            sys::MAP_PRIVATE | sys::MAP_ANONYMOUS | sys::MAP_NORESERVE,
            -1,
            0,
        )
    };
    if p == sys::MAP_FAILED {
        r.mode = 2;
    } else {
        r.base = p as usize;
        r.mode = 1;
    }
    r.mode
}

/// Allocates `size` bytes (the caller initializes the header).
///
/// # Safety
/// Returns uninitialized memory; aborts when out of memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_alloc(size: usize) -> *mut u8 {
    stat_alloc();
    // SAFETY: allocator internals.
    unsafe { raw_alloc(size) }
}

#[cfg(feature = "debug-rt")]
unsafe fn raw_alloc(size: usize) -> *mut u8 {
    // SAFETY: forwarded.
    unsafe { crate::debug::debug_alloc(size) }
}

#[cfg(not(feature = "debug-rt"))]
unsafe fn raw_alloc(size: usize) -> *mut u8 {
    // SAFETY: allocator internals; single-threaded.
    unsafe {
        let mode = {
            let m = REGION_STATE.get().mode;
            if m == 0 { init() } else { m }
        };
        if mode == 1 && size <= MAX_SMALL {
            let class = size.max(8).div_ceil(8) - 1;
            let lists = cirka_free_lists.get();
            let mut p = lists[class];
            if p.is_null() {
                refill(class);
                p = lists[class];
            }
            lists[class] = *(p as *mut *mut u8);
            return p;
        }
        let p = sys::malloc(size.max(8)) as *mut u8;
        if p.is_null() {
            crate::io::fatal(b"out of memory", 2);
        }
        p
    }
}

/// Carves a new page for `class` and pushes all its objects on the free list.
#[cfg(not(feature = "debug-rt"))]
unsafe fn refill(class: usize) {
    // SAFETY: single-threaded allocator state.
    unsafe {
        let r = REGION_STATE.get();
        if r.next_page >= PAGES {
            crate::io::fatal(b"out of memory (region exhausted)", 2);
        }
        let page = r.next_page;
        r.next_page += 1;
        PAGE_CLASS.get()[page] = class as u8;
        let size = 8 * (class + 1);
        let start = r.base + page * PAGE;
        let n = PAGE / size;
        let lists = cirka_free_lists.get();
        let mut head = lists[class];
        let mut i = n;
        while i > 0 {
            i -= 1;
            let p = (start + i * size) as *mut *mut u8;
            *p = head;
            head = p as *mut u8;
        }
        lists[class] = head;
    }
}

/// Returns an object's memory to the allocator.
///
/// # Safety
/// `o` must have been allocated by `cirka_alloc` and be dead.
pub unsafe fn dealloc(o: Obj) {
    #[cfg(feature = "debug-rt")]
    {
        // SAFETY: forwarded.
        unsafe { crate::debug::debug_free(o as *mut u8) };
    }
    #[cfg(not(feature = "debug-rt"))]
    // SAFETY: allocator internals; single-threaded.
    unsafe {
        let r = REGION_STATE.get();
        let a = o as usize;
        if r.mode == 1 && a >= r.base && a < r.base + REGION {
            let class = PAGE_CLASS.get()[(a - r.base) >> PAGE_SHIFT] as usize;
            let lists = cirka_free_lists.get();
            *(o as *mut *mut u8) = lists[class];
            lists[class] = o as *mut u8;
        } else {
            sys::free(o as *mut c_void);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_free_reuses_memory() {
        // SAFETY: test of allocator internals on one thread.
        unsafe {
            let a = cirka_alloc(24);
            let b = cirka_alloc(24);
            assert_ne!(a, b);
            dealloc(a as Obj);
            let c = cirka_alloc(24);
            if !cfg!(feature = "debug-rt") && REGION_STATE.get().mode == 1 {
                assert_eq!(a, c, "freed memory is reused for the same class");
            }
            let big = cirka_alloc(4096);
            *big = 1;
            dealloc(big as Obj);
            dealloc(b as Obj);
            dealloc(c as Obj);
        }
    }
}
