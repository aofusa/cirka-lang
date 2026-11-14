//! Allocators (05-runtime.md §3).
//!
//! - `CIRKA_ALLOC=cirka` (default): size classes of 8..=256 bytes served
//!   from per-thread free lists (05-runtime.md §9.2). Pages of 64 KiB are
//!   carved out of one large reserved region under a lock, and a per-page
//!   table records each page's size class so that `dealloc` can find the
//!   class of any object from its address alone. An object freed by
//!   another thread joins that thread's free list; the lists of a thread
//!   that exits go to a pool that `refill` takes from before carving a new
//!   page. Larger objects use `malloc`.
//! - `CIRKA_ALLOC=system`: `malloc`/`free` for everything (valgrind/ASan).
//!
//! The debug runtime (feature `debug-rt`) always uses `malloc` with a
//! prefix (see `debug.rs`).

#![cfg_attr(feature = "debug-rt", allow(dead_code, unused_imports))]

use crate::Global;
use crate::object::Obj;
use crate::sync::SpinLock;
use crate::sys;
use crate::tls;
use core::ffi::c_void;

pub const CLASSES: usize = 32;
const MAX_SMALL: usize = 8 * CLASSES;
const PAGE_SHIFT: usize = 16;
const PAGE: usize = 1 << PAGE_SHIFT;
const REGION: usize = 64 << 30;
const PAGES: usize = REGION / PAGE;

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

/// Free lists given back by threads that exited, per size class.
static POOL: Global<[*mut u8; CLASSES]> = Global::new([core::ptr::null_mut(); CLASSES]);

/// Guards `REGION_STATE.next_page`, `PAGE_CLASS` writes, `POOL` and
/// `TOTALS`.
static HEAP_LOCK: SpinLock = SpinLock::new();

/// Allocation statistics (printed with `CIRKA_STATS=1`), counted per thread.
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub allocs: u64,
    pub frees: u64,
    pub live: u64,
    pub max_live: u64,
    /// Thunks evaluated (`cirka_force`).
    pub forced: u64,
    /// Shared arrays copied by an in-place update (`cirka_array_unique`).
    pub array_copies: u64,
}

impl Stats {
    pub const ZERO: Stats = Stats {
        allocs: 0,
        frees: 0,
        live: 0,
        max_live: 0,
        forced: 0,
        array_copies: 0,
    };
}

/// The statistics of threads that exited.
static TOTALS: Global<Stats> = Global::new(Stats::ZERO);

/// This thread's statistics.
///
/// # Safety
/// The reference must not outlive the current thread or overlap another
/// reference obtained from this function.
#[allow(clippy::mut_from_ref)]
#[inline(always)]
pub unsafe fn stats() -> &'static mut Stats {
    // SAFETY: the calling thread's allocator state.
    unsafe { &mut (*tls::heap()).stats }
}

/// The statistics of all threads: those that exited and the current one.
pub fn total_stats() -> Stats {
    let _g = HEAP_LOCK.lock();
    // SAFETY: TOTALS under HEAP_LOCK; the current thread's own block.
    unsafe {
        let mut t = *TOTALS.get();
        add_stats(&mut t, stats());
        t
    }
}

fn add_stats(t: &mut Stats, s: &Stats) {
    t.allocs += s.allocs;
    t.frees += s.frees;
    t.live = t.live.wrapping_add(s.live);
    t.max_live += s.max_live;
    t.forced += s.forced;
    t.array_copies += s.array_copies;
}

/// Counts an allocation. A thread that frees objects of other threads may
/// have freed more than it allocated: `live` is then "negative" (wrapped).
#[inline(always)]
fn stat_alloc() {
    // SAFETY: this thread's statistics.
    let s = unsafe { stats() };
    s.allocs += 1;
    s.live = s.live.wrapping_add(1);
    if s.live as i64 > s.max_live as i64 {
        s.max_live = s.live;
    }
}

#[inline(always)]
pub fn stat_free() {
    // SAFETY: this thread's statistics.
    let s = unsafe { stats() };
    s.frees += 1;
    s.live = s.live.wrapping_sub(1);
}

/// Called by a thread that is about to exit: gives its free lists to the
/// pool and adds its statistics to the totals.
pub fn thread_exit() {
    let _g = HEAP_LOCK.lock();
    // SAFETY: POOL and TOTALS under HEAP_LOCK; the exiting thread's block,
    // which nothing else uses.
    unsafe {
        let t = &mut *tls::heap();
        let pool = POOL.get();
        for (class, head) in t.free_lists.iter_mut().enumerate() {
            if head.is_null() {
                continue;
            }
            // Append the pool's list to the end of this one.
            let mut last = *head;
            while !(*(last as *mut *mut u8)).is_null() {
                last = *(last as *mut *mut u8);
            }
            *(last as *mut *mut u8) = pool[class];
            pool[class] = *head;
            *head = core::ptr::null_mut();
        }
        add_stats(TOTALS.get(), &t.stats);
        t.stats = Stats::ZERO;
    }
}

/// Adds this thread's statistics to the totals and starts counting from
/// zero (a pool thread after each job, which never exits: its counts
/// must be in the totals printed at exit).
pub fn flush_stats() {
    let _g = HEAP_LOCK.lock();
    // SAFETY: TOTALS under HEAP_LOCK; this thread's own block.
    unsafe {
        let t = &mut (*tls::heap()).stats;
        add_stats(TOTALS.get(), t);
        *t = Stats::ZERO;
    }
}

#[cfg_attr(feature = "debug-rt", allow(dead_code))]
#[cold]
#[inline(never)]
fn init() -> u8 {
    let _g = HEAP_LOCK.lock();
    // SAFETY: under HEAP_LOCK (the first allocation happens on the main
    // thread before other threads exist; the lock covers unit tests).
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

/// Allocates `size` zeroed bytes (the caller initializes the header). Large
/// objects come from `calloc`, which skips writing memory fresh from the
/// system: it is then touched once, by its first use (FEAT-0068).
///
/// # Safety
/// Aborts when out of memory.
pub unsafe fn alloc_zeroed(size: usize) -> *mut u8 {
    stat_alloc();
    // SAFETY: allocator internals.
    unsafe { raw_alloc_zeroed(size) }
}

#[cfg(feature = "debug-rt")]
unsafe fn raw_alloc_zeroed(size: usize) -> *mut u8 {
    // SAFETY: a fresh allocation of `size` bytes.
    unsafe {
        let p = crate::debug::debug_alloc(size);
        core::ptr::write_bytes(p, 0, size);
        p
    }
}

#[cfg(not(feature = "debug-rt"))]
unsafe fn raw_alloc_zeroed(size: usize) -> *mut u8 {
    // SAFETY: a fresh allocation of `size` bytes. Large objects go through
    // `malloc` / `free` in both allocator modes, so `calloc` pairs with
    // `dealloc` before the allocator is initialized too.
    unsafe {
        if size <= MAX_SMALL {
            let p = raw_alloc(size);
            core::ptr::write_bytes(p, 0, size);
            return p;
        }
        let p = sys::calloc(1, size) as *mut u8;
        if p.is_null() {
            crate::io::fatal(b"out of memory", 2);
        }
        p
    }
}

#[cfg(feature = "debug-rt")]
unsafe fn raw_alloc(size: usize) -> *mut u8 {
    // SAFETY: forwarded.
    unsafe { crate::debug::debug_alloc(size) }
}

/// The fast path: a small object popped from this thread's free list.
#[cfg(not(feature = "debug-rt"))]
#[inline(always)]
unsafe fn raw_alloc(size: usize) -> *mut u8 {
    // SAFETY: allocator internals; the free lists are this thread's, and
    // `mode` is written once before any other thread exists.
    unsafe {
        if REGION_STATE.get().mode == 1 && size <= MAX_SMALL {
            // 8 <= max(size, 8) <= MAX_SMALL, so class < CLASSES.
            let class = size.max(8).div_ceil(8) - 1;
            let head = (*tls::heap()).free_lists.get_unchecked_mut(class);
            let p = *head;
            if !p.is_null() {
                *head = *(p as *mut *mut u8);
                return p;
            }
        }
        raw_alloc_slow(size)
    }
}

/// Every other allocation: the first one, an empty free list, a large
/// object, the system allocator. Out of line, so that the fast path saves
/// no registers.
#[cfg(not(feature = "debug-rt"))]
#[inline(never)]
unsafe fn raw_alloc_slow(size: usize) -> *mut u8 {
    // SAFETY: as for raw_alloc.
    unsafe {
        let mode = {
            let m = REGION_STATE.get().mode;
            if m == 0 { init() } else { m }
        };
        if mode == 1 && size <= MAX_SMALL {
            let class = size.max(8).div_ceil(8) - 1;
            let lists = &mut (*tls::heap()).free_lists;
            if lists[class].is_null() {
                refill(class);
            }
            let p = lists[class];
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

/// Refills this thread's empty free list of `class`: from the pool when
/// an exited thread left one, otherwise by carving a new page.
#[cfg(not(feature = "debug-rt"))]
#[cold]
#[inline(never)]
unsafe fn refill(class: usize) {
    let _g = HEAP_LOCK.lock();
    // SAFETY: shared allocator state under HEAP_LOCK; this thread's lists.
    unsafe {
        let lists = &mut (*tls::heap()).free_lists;
        let pool = POOL.get();
        if !pool[class].is_null() {
            lists[class] = pool[class];
            pool[class] = core::ptr::null_mut();
            return;
        }
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
    // SAFETY: allocator internals. The page's class was written (under
    // HEAP_LOCK) before the object was allocated, which happens before any
    // thread frees it; the free list is this thread's.
    unsafe {
        let r = REGION_STATE.get();
        let a = o as usize;
        if r.mode == 1 && a >= r.base && a < r.base + REGION {
            let class = PAGE_CLASS.get()[(a - r.base) >> PAGE_SHIFT] as usize;
            let lists = &mut (*tls::heap()).free_lists;
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
    fn zeroed_memory_is_zero_after_reuse() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        // SAFETY: test of allocator internals on one thread.
        unsafe {
            // A small object, a large one, and one `malloc` serves by `mmap`.
            for size in [24, 4096, 1 << 20] {
                let a = cirka_alloc(size);
                core::ptr::write_bytes(a, 0xAB, size);
                dealloc(a as Obj);
                let b = alloc_zeroed(size);
                assert!((0..size).all(|i| *b.add(i) == 0), "size {size}");
                dealloc(b as Obj);
            }
        }
    }

    #[test]
    fn alloc_free_reuses_memory() {
        let _g = crate::TEST_LOCK.lock().unwrap();
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
