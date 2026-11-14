//! Object representation (05-runtime.md §1) and reference counting.
//!
//! ```text
//!  0        4      6      7      8
//!  +--------+------+------+------+----------------------------
//!  | rc:i32 | tag  | nptr | kind | fields...
//!  +--------+------+------+------+----------------------------
//! ```
//!
//! `rc > 0`: single-threaded count, `rc < 0`: shared (atomic, |rc| is the
//! count), `rc == 0`: immortal (static data, CAFs, regions).

use crate::alloc::{dealloc, stat_free};
use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

#[repr(C)]
pub struct Header {
    pub rc: i32,
    pub tag: u16,
    pub nptr: u8,
    pub kind: u8,
}

pub type Obj = *mut Header;

pub const CON: u8 = 0;
pub const FUN: u8 = 1;
pub const PAP: u8 = 2;
pub const THUNK: u8 = 3;
pub const BLACKHOLE: u8 = 4;
pub const IND: u8 = 5;
pub const ARRAY: u8 = 6;
pub const BYTES: u8 = 7;
pub const BIGINT: u8 = 8;
pub const FOREIGN: u8 = 9;
pub const MUTVAR: u8 = 10;
pub const MVAR: u8 = 11;

/// The reference count of `o`. An atomic load (a plain `mov` on x86-64):
/// another thread may be changing the count of a shared object meanwhile.
///
/// # Safety
/// `o` must be a live object.
#[inline(always)]
pub unsafe fn rc(o: Obj) -> i32 {
    // SAFETY: the count is only ever accessed as an aligned i32.
    unsafe { AtomicI32::from_ptr(&raw mut (*o).rc).load(Ordering::Relaxed) }
}

/// The kind of `o`, read with Acquire: a thunk shared between threads
/// becomes an indirection whose value word was written before (§9.5).
///
/// # Safety
/// `o` must be a live object.
#[inline(always)]
pub unsafe fn kind_acquire(o: Obj) -> u8 {
    // SAFETY: the kind is only ever accessed as a u8.
    unsafe { AtomicU8::from_ptr(&raw mut (*o).kind).load(Ordering::Acquire) }
}

/// Word `i` after the header (offset 8 + 8*i).
///
/// # Safety
/// `o` must point to a live object with at least `i + 1` words.
#[inline(always)]
pub unsafe fn word(o: Obj, i: usize) -> *mut usize {
    // SAFETY: guaranteed by the caller.
    unsafe { (o as *mut u8).add(8 + 8 * i) as *mut usize }
}

/// The range of RC-managed pointer words of an object: (first word, count).
///
/// # Safety
/// `o` must point to a live object whose header has the given kind / nptr.
#[inline]
pub unsafe fn ptr_fields(o: Obj, kind: u8, nptr: u8) -> (usize, usize) {
    match kind {
        CON | PAP => (0, nptr as usize),
        IND | MUTVAR => (0, 1),
        FUN | THUNK | BLACKHOLE => (1, nptr as usize),
        // SAFETY: arrays store their length in word 0.
        ARRAY => (1, unsafe { *word(o, 0) }),
        _ => (0, 0),
    }
}

// ---- iterative freeing ---------------------------------------------------
//
// Dead objects awaiting the release of their children are linked through
// their own (no longer needed) header: `next >> 3` (44 bits, user-space
// pointers are below 2^47 and 8-aligned), kind (4 bits) and nptr (8 bits).
// The rest of the object (e.g. the array length) stays intact. No extra
// memory is needed, so freeing a list of any length uses constant stack.

#[inline(always)]
fn pack(next: Obj, kind: u8, nptr: u8) -> u64 {
    ((next as u64 >> 3) << 12) | ((kind as u64 & 0xf) << 8) | nptr as u64
}

#[inline(always)]
fn unpack(h: u64) -> (Obj, u8, u8) {
    (((h >> 12) << 3) as Obj, ((h >> 8) & 0xf) as u8, h as u8)
}

/// Decrements `c`; returns true when it dies (count reached zero).
///
/// # Safety
/// `c` must be a live object.
#[inline(always)]
pub(crate) unsafe fn dec_dies(c: Obj) -> bool {
    // SAFETY: c is live; a positive count is only touched by this thread.
    unsafe {
        let rc = rc(c);
        if rc == 1 {
            true
        } else if rc > 1 {
            (*c).rc = rc - 1;
            false
        } else if rc < 0 {
            AtomicI32::from_ptr(&raw mut (*c).rc).fetch_add(1, Ordering::AcqRel) == -1
        } else {
            false
        }
    }
}

/// Frees an object whose count has reached zero, and (iteratively) every
/// object that dies as a consequence.
///
/// # Safety
/// `o` must be a heap object that is no longer referenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_free_object(o: Obj) {
    // SAFETY: every object on the list is dead and owned by this routine.
    unsafe {
        crate::debug::check_live(o, "free");
        *(o as *mut u64) = pack(core::ptr::null_mut(), (*o).kind, (*o).nptr);
        let mut head = o;
        while !head.is_null() {
            let x = head;
            let (next, kind, nptr) = unpack(*(x as *mut u64));
            head = next;
            let (start, count) = ptr_fields(x, kind, nptr);
            for i in start..start + count {
                let c = *word(x, i) as Obj;
                if c.is_null() {
                    continue;
                }
                crate::debug::check_live(c, "drop");
                if dec_dies(c) {
                    *(c as *mut u64) = pack(head, (*c).kind, (*c).nptr);
                    head = c;
                }
            }
            release(x, kind);
        }
    }
}

/// Releases the memory of a dead object of kind `kind` whose pointer
/// fields have been dealt with (running a `FOREIGN` finalizer first).
///
/// # Safety
/// `x` must be a dead heap object that nothing refers to any more.
pub(crate) unsafe fn release(x: Obj, kind: u8) {
    // SAFETY: guaranteed by the caller; a FOREIGN object stores its
    // pointer in word 0 and its finalizer (or 0) in word 1; an MVAR object
    // owns its synchronization box.
    unsafe {
        if kind >= FOREIGN {
            release_resources(x, kind);
        }
        stat_free();
        dealloc(x);
    }
}

/// Runs the finalizer of a dead `FOREIGN` object, or frees the
/// synchronization box of a dead `MVAR`: out of line, so that freeing
/// other objects tests the kind once.
///
/// # Safety
/// As for `release`.
#[inline(never)]
unsafe fn release_resources(x: Obj, kind: u8) {
    // SAFETY: as for release.
    unsafe {
        if kind == FOREIGN {
            let fin = *word(x, 1);
            if fin != 0 {
                let f: extern "C" fn(usize) = core::mem::transmute(fin);
                f(*word(x, 0));
            }
        } else if kind == MVAR {
            crate::sync::release(x);
        }
    }
}

/// `dup`: RC + 1.
///
/// # Safety
/// `o` must be a live object (or a static one).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_dup(o: Obj) {
    // SAFETY: o is live.
    unsafe {
        crate::debug::check_live(o, "dup");
        let rc = rc(o);
        if rc > 0 {
            (*o).rc = rc + 1;
        } else if rc < 0 {
            AtomicI32::from_ptr(&raw mut (*o).rc).fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// `drop`: RC - 1, freeing the object when it reaches zero.
///
/// # Safety
/// `o` must be a live object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_drop(o: Obj) {
    // SAFETY: o is live and owned.
    unsafe {
        crate::debug::check_live(o, "drop");
        if dec_dies(o) {
            cirka_free_object(o);
        }
    }
}

/// Slow path of `drop` for shared (atomic) objects.
///
/// # Safety
/// `o` must be a live shared object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_dec_ref_slow(o: Obj) {
    // SAFETY: as for cirka_drop.
    unsafe { cirka_drop(o) }
}

/// Drops the pointer fields of `o` without freeing `o` itself (used by
/// `drop_reuse` when the object is unique).
///
/// # Safety
/// `o` must be a live, uniquely owned object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_drop_fields(o: Obj) {
    // SAFETY: o is live and unique.
    unsafe {
        let (start, count) = ptr_fields(o, (*o).kind, (*o).nptr);
        for i in start..start + count {
            let c = *word(o, i) as Obj;
            if !c.is_null() {
                cirka_drop(c);
            }
        }
    }
}

/// Frees the memory of an object without touching its fields (unused reuse
/// tokens, closures whose captured values were moved out).
///
/// # Safety
/// `o` must be a heap object with no remaining references.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_free_raw(o: Obj) {
    if o.is_null() {
        return;
    }
    // SAFETY: o is a dead heap object.
    unsafe {
        crate::debug::check_live(o, "free");
        stat_free();
        dealloc(o);
    }
}

/// `drop_reuse`: when `o` is unique its fields are dropped and its memory
/// is returned as a reuse token; otherwise the count is decremented and
/// null is returned.
///
/// # Safety
/// `o` must be a live object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_drop_reuse(o: Obj) -> Obj {
    // SAFETY: o is live and owned.
    unsafe {
        crate::debug::check_live(o, "drop_reuse");
        if rc(o) == 1 {
            cirka_drop_fields(o);
            o
        } else {
            cirka_drop(o);
            core::ptr::null_mut()
        }
    }
}

/// Marks `o` and everything reachable from it as shared between threads
/// (negative counts, atomic operations from now on; 05-runtime.md §9.3).
/// Objects already shared (negative) or immortal (zero) are not entered:
/// what they reach is shared or immortal too. The work stack grows on the
/// heap, so deep structures do not use the C stack.
///
/// The objects being marked are not yet visible to another thread, so the
/// marking itself needs no atomic operations.
///
/// # Safety
/// `o` must be a live object (or null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mark_shared(o: Obj) {
    // SAFETY: traversal of live objects; the mark is the sign of rc.
    unsafe {
        let mut stack = Stack::new();
        stack.push(o);
        while let Some(x) = stack.pop() {
            if x.is_null() {
                continue;
            }
            let n = rc(x);
            if n <= 0 {
                continue;
            }
            (*x).rc = -n;
            if (*x).kind == MVAR {
                crate::sync::for_each_value(x, |v| stack.push(v));
                continue;
            }
            let (start, count) = ptr_fields(x, (*x).kind, (*x).nptr);
            for i in start..start + count {
                stack.push(*word(x, i) as Obj);
            }
        }
    }
}

/// A work stack of objects: 64 entries inline, then `malloc`.
struct Stack {
    inline: [Obj; 64],
    heap: *mut Obj,
    cap: usize,
    len: usize,
}

impl Stack {
    fn new() -> Stack {
        Stack {
            inline: [core::ptr::null_mut(); 64],
            heap: core::ptr::null_mut(),
            cap: 64,
            len: 0,
        }
    }

    fn slots(&mut self) -> *mut Obj {
        if self.heap.is_null() {
            self.inline.as_mut_ptr()
        } else {
            self.heap
        }
    }

    fn push(&mut self, o: Obj) {
        if self.len == self.cap {
            let cap = self.cap * 2;
            // SAFETY: a fresh buffer of `cap` entries receiving the `len`
            // current ones; the old heap buffer (if any) is released.
            unsafe {
                let p = crate::sys::malloc(cap * 8) as *mut Obj;
                if p.is_null() {
                    crate::io::fatal(b"out of memory", 2);
                }
                core::ptr::copy_nonoverlapping(self.slots(), p, self.len);
                if !self.heap.is_null() {
                    crate::sys::free(self.heap as *mut core::ffi::c_void);
                }
                self.heap = p;
            }
            self.cap = cap;
        }
        // SAFETY: `len < cap` entries.
        unsafe { *self.slots().add(self.len) = o };
        self.len += 1;
    }

    fn pop(&mut self) -> Option<Obj> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: `len` was a filled entry.
        Some(unsafe { *self.slots().add(self.len) })
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if !self.heap.is_null() {
            // SAFETY: the buffer came from malloc.
            unsafe { crate::sys::free(self.heap as *mut core::ffi::c_void) };
        }
    }
}
