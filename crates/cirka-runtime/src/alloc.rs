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
//! In both modes, arrays of 2 MiB or more go on transparent huge pages
//! where the system has them (`advise_huge`, FEAT-1011; `CIRKA_THP=0`
//! keeps them on ordinary pages): the zero-filled objects of
//! `alloc_zeroed` (unboxed arrays) and boxed arrays. Buffers grown with
//! `realloc` (builders) and the other blocks from `malloc` keep small
//! pages.
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
use core::sync::atomic::{AtomicU8, Ordering};

pub const CLASSES: usize = 32;
/// The largest object served from the region (larger ones come from
/// `malloc`).
pub(crate) const MAX_SMALL: usize = 8 * CLASSES;
const PAGE_SHIFT: usize = 16;
const PAGE: usize = 1 << PAGE_SHIFT;
const REGION: usize = 64 << 30;
/// The smallest region `init` reserves when the address space is limited
/// (valgrind, `ulimit -v`) before it falls back to the system allocator.
const MIN_REGION: usize = 1 << 30;
const PAGES: usize = REGION / PAGE;

/// The region and the size class of each of its pages, in one static with
/// the table last: what an allocation or a free reads (`mode`, `base`,
/// `size`, the class of a page of a small heap) lies in one cache line
/// whatever order the linker gives the statics (FEAT-1026).
#[repr(C, align(64))]
struct Region {
    base: usize,
    /// Bytes reserved (at most `REGION`); 0 unless the region is in use
    /// (mode 1), so that [`in_region`] needs no test of the mode.
    size: usize,
    next_page: usize,
    /// 0 = not initialized, 1 = cirka allocator, 2 = system allocator.
    mode: u8,
    /// The size class of each page handed out (`refill`).
    class: [u8; PAGES],
}

static REGION_STATE: Global<Region> = Global::new(Region {
    base: 0,
    size: 0,
    next_page: 0,
    mode: 0,
    class: [0; PAGES],
});

/// Free lists given back by threads that exited, per size class.
static POOL: Global<[*mut u8; CLASSES]> = Global::new([core::ptr::null_mut(); CLASSES]);

/// Guards `REGION_STATE.next_page`, writes of `REGION_STATE.class`,
/// `POOL` and `TOTALS`.
static HEAP_LOCK: SpinLock = SpinLock::new();

/// Allocation statistics (printed with `CIRKA_STATS=1`), counted per thread.
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub allocs: u64,
    pub frees: u64,
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
        max_live: 0,
        forced: 0,
        array_copies: 0,
    };

    /// Objects allocated and not freed: kept as the difference, so that a
    /// free counts with one increment (FEAT-1026). A thread that frees
    /// objects of other threads may have freed more than it allocated:
    /// the value is then "negative" (wrapped).
    pub fn live(&self) -> u64 {
        self.allocs.wrapping_sub(self.frees)
    }

    /// The highest number of live objects so far: `max_live` is recorded
    /// when an object is freed ([`stat_free`]), which follows every peak
    /// unless the objects are still live now.
    pub fn peak(&self) -> u64 {
        let live = self.live();
        if live as i64 > self.max_live as i64 {
            live
        } else {
            self.max_live
        }
    }
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
    t.max_live += s.peak();
    t.forced += s.forced;
    t.array_copies += s.array_copies;
}

/// Counts an allocation: one increment, as the generated code's inline
/// allocation does (FEAT-1027). The number of live objects only falls at
/// a free, so its highest value is recorded there ([`stat_free`], and
/// [`Stats::peak`] for the objects still live).
#[inline(always)]
pub(crate) fn stat_alloc() {
    // SAFETY: this thread's statistics.
    unsafe { stats().allocs += 1 };
}

/// Counts a free, recording the number of live objects before it when it
/// is the highest so far.
#[inline(always)]
pub fn stat_free() {
    // SAFETY: this thread's statistics.
    let s = unsafe { stats() };
    let live = s.live();
    if live as i64 > s.max_live as i64 {
        s.max_live = live;
    }
    s.frees += 1;
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
    // SAFETY: anonymous private mappings; MAP_NORESERVE only reserves
    // address space, physical pages are committed on first touch.
    let region = reserve(REGION, MIN_REGION, |size| unsafe {
        sys::mmap(
            core::ptr::null_mut(),
            size,
            sys::PROT_READ | sys::PROT_WRITE,
            sys::MAP_PRIVATE | sys::MAP_ANONYMOUS | sys::MAP_NORESERVE,
            -1,
            0,
        )
    });
    match region {
        Some((base, size)) => {
            r.base = base;
            r.size = size;
            r.mode = 1;
        }
        None => r.mode = 2,
    }
    r.mode
}

/// The largest mapping `map` gives of `max` bytes, then half as many, down
/// to `min` (BUG-0042): where the address space is limited (valgrind
/// refuses 64 GiB, `ulimit -v`), a smaller region keeps the size classes
/// instead of `malloc` for every object. Its base and size.
#[cfg_attr(feature = "debug-rt", allow(dead_code))]
fn reserve(
    max: usize,
    min: usize,
    mut map: impl FnMut(usize) -> *mut c_void,
) -> Option<(usize, usize)> {
    let mut size = max;
    loop {
        let p = map(size);
        if p != sys::MAP_FAILED {
            return Some((p as usize, size));
        }
        if size / 2 < min {
            return None;
        }
        size /= 2;
    }
}

/// Allocates `size` bytes (the caller initializes the header).
///
/// # Safety
/// Returns uninitialized memory; aborts when out of memory.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.hot.cirka_alloc")]
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
        advise_huge(p, size);
        p
    }
}

#[cfg(feature = "debug-rt")]
unsafe fn raw_alloc(size: usize) -> *mut u8 {
    // SAFETY: forwarded.
    unsafe { crate::debug::debug_alloc(size) }
}

/// The fast path: a small object popped from this thread's free list. The
/// lists hold objects only while the region is in use (mode 1: `refill`
/// and `dealloc` fill them with objects of the region, the system
/// allocator never), so an empty one covers the other modes: the
/// generated code's inline allocation is the same (FEAT-1027).
#[cfg(not(feature = "debug-rt"))]
#[inline(always)]
unsafe fn raw_alloc(size: usize) -> *mut u8 {
    // SAFETY: allocator internals; the free lists are this thread's.
    unsafe {
        if size <= MAX_SMALL {
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

/// The size of a transparent huge page (a PMD on x86-64).
const HUGE: usize = 2 << 20;

/// Whether objects of `HUGE` bytes or more go on transparent huge pages:
/// 0 = `CIRKA_THP` not read yet, 1 = yes, 2 = no (`CIRKA_THP=0`).
static THP: AtomicU8 = AtomicU8::new(0);

/// The part of the `size` bytes at `p` that whole huge pages cover: from
/// `p` rounded up to `HUGE` to `p + size` rounded down, as (start, length);
/// `None` when no huge page fits.
fn huge_range(p: usize, size: usize) -> Option<(usize, usize)> {
    let start = p.checked_add(HUGE - 1)? & !(HUGE - 1);
    let end = p.checked_add(size)? & !(HUGE - 1);
    (start < end).then(|| (start, end - start))
}

/// Asks the kernel to back the huge pages inside an array of `size` bytes
/// at `p`, just allocated by `malloc` or `calloc`, with transparent huge
/// pages (FEAT-1011, 05-runtime.md §3): a page fault per 2 MiB instead of
/// per 4 KiB, and fewer TLB misses in the loops over the array. The first
/// block of a block from `mmap` keeps small pages (glibc wrote the header
/// of the block there). Without transparent huge pages (the system setting
/// `never`, an old kernel) `madvise` fails or does nothing, which is fine.
///
/// Only arrays: a buffer written once (a builder grown with `realloc`, a
/// string) loses more than it gains, since the kernel zeroes a whole huge
/// page at its first fault, which evicts the cache the writes would hit
/// (cli-grep and string-split were 6 % slower).
pub(crate) fn advise_huge(p: *mut u8, size: usize) {
    if size < HUGE || !thp_enabled() {
        return;
    }
    if let Some((start, len)) = huge_range(p as usize, size) {
        // SAFETY: the range lies inside the `size` bytes at `p`, which the
        // caller owns; the advice changes no contents.
        unsafe {
            sys::madvise(start as *mut c_void, len, sys::MADV_HUGEPAGE);
        }
    }
}

/// `CIRKA_THP` (read at the first large object): huge pages unless `0`.
fn thp_enabled() -> bool {
    match THP.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let on = thp_wanted(sys::env(c"CIRKA_THP"));
            THP.store(if on { 1 } else { 2 }, Ordering::Relaxed);
            on
        }
    }
}

/// The meaning of the value of `CIRKA_THP`: anything but `0` (unset,
/// empty, `1`) wants huge pages.
fn thp_wanted(v: Option<&[u8]>) -> bool {
    v != Some(b"0")
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
        if r.next_page >= r.size / PAGE {
            crate::io::fatal(b"out of memory (region exhausted)", 2);
        }
        let page = r.next_page;
        r.next_page += 1;
        r.class[page] = class as u8;
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

/// Returns an object's memory to the allocator. Always inlined: it is the
/// hot end of every free (`release`, `cirka_free_raw`), and left to the
/// inliner it stayed a call there or not depending on unrelated code in
/// the crate (FEAT-0096: 8 % on binary-trees).
///
/// # Safety
/// `o` must have been allocated by `cirka_alloc` and be dead.
#[inline(always)]
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
        if in_region(a, r.base, r.size) {
            // a - base < size <= REGION: the page is in the table, and a
            // page of the region has a class below CLASSES (`refill`).
            let class = *r.class.get_unchecked((a - r.base) >> PAGE_SHIFT) as usize;
            let heap = tls::heap();
            let head = (*heap).free_lists.get_unchecked_mut(class);
            *(o as *mut *mut u8) = *head;
            *head = o as *mut u8;
            // The region is in use: the generated code may free objects of
            // sizes it knows onto this thread's lists itself (FEAT-1027).
            (*heap).free_fast = 1;
        } else {
            sys::free(o as *mut c_void);
        }
    }
}

/// Whether address `a` is in the region of `size` bytes at `base`: one
/// unsigned comparison, an address below the base wrapping around to a
/// large offset. `size` is 0 when the region is not in use, so the test
/// needs no load of the mode (BUG-0043: the free of every object makes it).
#[inline(always)]
fn in_region(a: usize, base: usize, size: usize) -> bool {
    a.wrapping_sub(base) < size
}

/// Resizes the memory of `o`, an object from `malloc`, to `size` bytes
/// with `realloc`, keeping its first bytes (glibc moves a block served by
/// `mmap` with `mremap`: neither copying nor touching the pages again).
/// `None` when `o` is in the region of small objects or the checked
/// runtime tracks it: the caller then copies.
///
/// # Safety
/// `o` must be a live object owned by the caller alone (no other reference
/// to it exists, so it may move).
pub unsafe fn resize_system(o: Obj, size: usize) -> Option<Obj> {
    #[cfg(feature = "debug-rt")]
    {
        let _ = (o, size);
        None
    }
    #[cfg(not(feature = "debug-rt"))]
    // SAFETY: an object outside the region came from `malloc` / `calloc`
    // (both allocator modes serve large objects that way, and every object
    // in mode 2); it is not in use elsewhere.
    unsafe {
        let r = REGION_STATE.get();
        if in_region(o as usize, r.base, r.size) {
            return None;
        }
        let p = sys::realloc(o as *mut c_void, size.max(8));
        if p.is_null() {
            crate::io::fatal(b"out of memory", 2);
        }
        Some(p as Obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The membership test of the region (BUG-0043): the bounds, an
    /// address below the base (which wraps around), and an empty region
    /// (not in use) that holds nothing.
    #[test]
    fn region_membership_is_one_comparison() {
        let base = 0x7f00_0000_0000usize;
        let size = 8usize << 30;
        assert!(in_region(base, base, size));
        assert!(in_region(base + size - 8, base, size));
        assert!(!in_region(base + size, base, size));
        assert!(!in_region(base - 8, base, size));
        assert!(!in_region(0, base, size));
        assert!(!in_region(usize::MAX, base, size));
        assert!(!in_region(base, base, 0));
        assert!(!in_region(0x1000, 0, 0));
    }

    /// The part of an object that whole huge pages cover (FEAT-1011).
    #[test]
    fn huge_range_is_the_aligned_interior() {
        let m = HUGE;
        // Smaller than a huge page, or no whole block inside.
        assert_eq!(huge_range(16 * m, m - 1), None);
        assert_eq!(huge_range(16 * m + 16, m), None);
        // An aligned start: every whole block.
        assert_eq!(huge_range(16 * m, m), Some((16 * m, m)));
        assert_eq!(huge_range(16 * m, 3 * m + 5), Some((16 * m, 3 * m)));
        // The block of a `malloc` header (16 bytes in) is left out.
        assert_eq!(huge_range(16 * m + 16, 4 * m), Some((17 * m, 3 * m)));
        // No overflow at the top of the address space.
        assert_eq!(huge_range(usize::MAX - 8, 4 * m), None);
    }

    #[test]
    fn cirka_thp_turns_huge_pages_off_with_0() {
        assert!(thp_wanted(None));
        assert!(thp_wanted(Some(b"")));
        assert!(thp_wanted(Some(b"1")));
        assert!(!thp_wanted(Some(b"0")));
    }

    /// The flags (`VmFlags`) of the mapping of this process that contains
    /// address `a`.
    fn vm_flags(a: usize) -> Vec<String> {
        let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
        let mut inside = false;
        for line in smaps.lines() {
            let range = line.split(' ').next().unwrap_or("");
            if let Some((lo, hi)) = range.split_once('-')
                && let (Ok(lo), Ok(hi)) =
                    (usize::from_str_radix(lo, 16), usize::from_str_radix(hi, 16))
            {
                inside = lo <= a && a < hi;
            } else if inside && let Some(flags) = line.strip_prefix("VmFlags:") {
                return flags.split_whitespace().map(String::from).collect();
            }
        }
        panic!("no mapping contains {a:#x}");
    }

    /// Whether the whole blocks of the `size` bytes at `p` carry the
    /// huge-page advice (`hg`).
    fn advised(p: *const u8, size: usize) -> bool {
        let (start, _) = huge_range(p as usize, size).unwrap();
        vm_flags(start).iter().any(|f| f == "hg")
    }

    /// Whether a fresh zero-filled object of 8 huge pages is advised.
    fn large_object_is_advised() -> bool {
        // SAFETY: test of allocator internals; the object is freed.
        unsafe {
            let size = 8 * HUGE;
            let p = alloc_zeroed(size);
            let a = advised(p, size);
            dealloc(p as Obj);
            a
        }
    }

    /// Boxed arrays are advised too; a large block from `malloc` and a
    /// buffer grown with `realloc` (a builder) keep small pages (FEAT-1011:
    /// a buffer written once loses more than it gains).
    #[test]
    fn only_arrays_get_huge_pages() {
        let _g = crate::test_lock();
        if cfg!(feature = "debug-rt")
            || !std::path::Path::new("/sys/kernel/mm/transparent_hugepage").exists()
            || !thp_enabled()
        {
            return;
        }
        // SAFETY: test of allocator internals; every object is freed, and
        // the elements of the array are the static empty string.
        unsafe {
            let n = HUGE;
            let a = crate::cirka_array_new(n as i64, crate::empty_bytes());
            assert!(advised(a as *const u8, 16 + 8 * n));
            crate::object::cirka_drop(a);
            // Larger than glibc's largest dynamic `mmap` threshold (32 MiB),
            // so these are fresh mappings, not heap memory that an earlier
            // test may have advised.
            let size = 20 * HUGE;
            let p = cirka_alloc(size);
            assert!(!advised(p, size));
            dealloc(p as Obj);
            let b = crate::bytes::resize(crate::cirka_bytes_new(1000), size, false);
            assert!(!advised(b as *const u8, crate::bytes::bytes_size(size)));
            crate::object::cirka_drop(b);
        }
    }

    /// Objects of 2 MiB or more are advised to use transparent huge pages,
    /// and not with `CIRKA_THP=0` (FEAT-1011): the latter in a child
    /// process, since the setting is read once.
    #[test]
    fn large_objects_get_huge_pages_unless_turned_off() {
        let _g = crate::test_lock();
        if cfg!(feature = "debug-rt")
            || !std::path::Path::new("/sys/kernel/mm/transparent_hugepage").exists()
        {
            return;
        }
        assert_eq!(large_object_is_advised(), thp_enabled());
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "alloc::tests::thp_child",
                "--ignored",
                "--nocapture",
            ])
            .env("CIRKA_THP", "0")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && text.contains("1 passed"), "{text}");
    }

    /// Run by `large_objects_get_huge_pages_unless_turned_off` with
    /// `CIRKA_THP=0`: no advice. Alone, it checks the setting of the
    /// environment it is given.
    #[test]
    #[ignore]
    fn thp_child() {
        let _g = crate::test_lock();
        if cfg!(feature = "debug-rt") {
            return;
        }
        let off = std::env::var("CIRKA_THP").as_deref() == Ok("0");
        assert_eq!(thp_enabled(), !off);
        assert_eq!(large_object_is_advised(), !off);
    }

    #[test]
    fn zeroed_memory_is_zero_after_reuse() {
        let _g = crate::test_lock();
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
        let _g = crate::test_lock();
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
    /// Where the address space is limited, the region is the largest half,
    /// quarter, ... of 64 GiB that is given, down to the minimum (BUG-0042).
    #[test]
    fn a_smaller_region_when_the_large_one_is_refused() {
        let mut tried = std::vec::Vec::new();
        let got = reserve(REGION, MIN_REGION, |size| {
            tried.push(size);
            if size <= 8 << 30 {
                0x1000_0000 as *mut c_void
            } else {
                sys::MAP_FAILED
            }
        });
        assert_eq!(got, Some((0x1000_0000, 8 << 30)));
        assert_eq!(tried, [64 << 30, 32 << 30, 16 << 30, 8 << 30]);
        let none = reserve(REGION, MIN_REGION, |_| sys::MAP_FAILED);
        assert_eq!(none, None);
    }
}
