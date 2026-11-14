//! The cycle collector (`--cycles=collect`, 04-memory-model.md §5).
//!
//! Reference counting cannot free cyclic garbage. Objects that may close a
//! cycle (the thunks of recursive value bindings and `MUTVAR`s) are
//! recorded as *roots* by `cirka_cycle_candidate`; the root buffer holds a
//! reference to each. When the buffer fills up (and at exit) the collector
//! runs trial deletion (Bacon–Rajan) over the subgraph reachable from the
//! roots only — it is not a tracing collector of the whole heap:
//!
//! 1. *mark gray*: remove the references inside the subgraph from the counts;
//! 2. *scan*: an object whose count stays positive is referenced from
//!    outside; it and everything it reaches are *black* (counts restored);
//!    the others are *white*;
//! 3. *collect*: free the white objects. Surviving roots stay roots.
//!
//! Colors live in the two high bits of the header's `kind` byte while the
//! collector runs. Static (count 0) and shared (count < 0) objects take no
//! part. The collector runs synchronously when a root is recorded; every
//! reference held by running code is counted then (Perceus), so the
//! objects it uses are black.

use crate::Global;
use crate::object::{self, Obj, ptr_fields, word};
use crate::{io, sys};

/// Initial number of roots that triggers a collection.
const LIMIT: usize = 1024;

const GRAY: u8 = 0x40;
const BLACK: u8 = 0x80;
const WHITE: u8 = 0xC0;
const COLOR: u8 = 0xC0;

/// A growable array of object pointers in `malloc`ed memory (not runtime
/// objects, so they never show up as leaks).
struct PtrVec {
    ptr: *mut Obj,
    len: usize,
    cap: usize,
}

impl PtrVec {
    const fn new() -> PtrVec {
        PtrVec {
            ptr: core::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    fn push(&mut self, o: Obj) {
        if self.len == self.cap {
            let cap = (self.cap * 2).max(64);
            // SAFETY: a fresh buffer of `cap` pointers; the old contents are
            // copied before the old buffer is freed.
            unsafe {
                let p = sys::malloc(cap * size_of::<Obj>()) as *mut Obj;
                if p.is_null() {
                    io::ewrite(b"cirka: out of memory in the cycle collector\n");
                    sys::abort();
                }
                if self.len > 0 {
                    core::ptr::copy_nonoverlapping(self.ptr, p, self.len);
                }
                if !self.ptr.is_null() {
                    sys::free(self.ptr as *mut core::ffi::c_void);
                }
                self.ptr = p;
            }
            self.cap = cap;
        }
        // SAFETY: len < cap.
        unsafe { *self.ptr.add(self.len) = o };
        self.len += 1;
    }

    fn pop(&mut self) -> Option<Obj> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: len < cap and the slot was written.
        Some(unsafe { *self.ptr.add(self.len) })
    }

    fn get(&self, i: usize) -> Obj {
        assert!(i < self.len);
        // SAFETY: i < len.
        unsafe { *self.ptr.add(i) }
    }
}

struct State {
    enabled: bool,
    roots: PtrVec,
    limit: usize,
    /// Scratch space of a collection.
    old: PtrVec,
    seen: PtrVec,
    work: PtrVec,
    black: PtrVec,
}

static STATE: Global<State> = Global::new(State {
    enabled: false,
    roots: PtrVec::new(),
    limit: LIMIT,
    old: PtrVec::new(),
    seen: PtrVec::new(),
    work: PtrVec::new(),
    black: PtrVec::new(),
});

/// Enables the collector (called first by a program built with
/// `--cycles=collect`).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_cycles_enable() {
    // SAFETY: single-threaded runtime state.
    unsafe { STATE.get().enabled = true };
}

/// Records `o` (borrowed) as a root of the collector; may collect.
///
/// # Safety
/// `o` must be a live, fully initialized object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_cycle_candidate(o: Obj) {
    // SAFETY: `o` is live. The collector's state is used by the thread
    // running `main` only: no roots are recorded once the process is
    // multithreaded (05-runtime.md §9.2).
    unsafe {
        let s = STATE.get();
        if !s.enabled || crate::sync::is_mt() || object::rc(o) <= 0 {
            return;
        }
        (*o).rc += 1;
        s.roots.push(o);
        if s.roots.len >= s.limit {
            collect(s);
            s.limit = LIMIT.max(2 * s.roots.len);
        }
    }
}

/// At exit: collects, then releases the references to the surviving roots
/// (not while other threads run: the roots' objects may be in use).
pub fn finish() {
    // SAFETY: no other thread runs; the collector's state.
    unsafe {
        let s = STATE.get();
        if !s.enabled || crate::thread::running() > 0 {
            return;
        }
        collect(s);
        while let Some(r) = s.roots.pop() {
            object::cirka_drop(r);
        }
    }
}

#[inline(always)]
unsafe fn color(o: Obj) -> u8 {
    // SAFETY: o is live.
    unsafe { (*o).kind & COLOR }
}

#[inline(always)]
unsafe fn set_color(o: Obj, c: u8) {
    // SAFETY: o is live (and not static: statics are never colored).
    unsafe { (*o).kind = ((*o).kind & !COLOR) | c }
}

/// Calls `f` on each child of `o` that takes part in the collection:
/// colored, or not yet colored with a positive (single-threaded) count.
#[inline(always)]
unsafe fn each_child(o: Obj, mut f: impl FnMut(Obj)) {
    // SAFETY: o is live; its pointer words are null or live objects.
    unsafe {
        let (start, count) = ptr_fields(o, (*o).kind & !COLOR, (*o).nptr);
        for i in start..start + count {
            let c = *word(o, i) as Obj;
            if !c.is_null() && (color(c) != 0 || object::rc(c) > 0) {
                f(c);
            }
        }
    }
}

/// One collection (see the module documentation).
unsafe fn collect(s: &mut State) {
    // SAFETY: every pointer handled is a live object reachable from a root
    // (which the buffer keeps alive) until it is freed in the last step.
    unsafe {
        core::mem::swap(&mut s.roots, &mut s.old);
        let roots = &s.old;
        // The buffer's own references.
        for i in 0..roots.len {
            (*roots.get(i)).rc -= 1;
        }
        // Mark gray.
        for i in 0..roots.len {
            let r = roots.get(i);
            if color(r) == 0 {
                set_color(r, GRAY);
                s.seen.push(r);
                s.work.push(r);
            }
        }
        while let Some(p) = s.work.pop() {
            each_child(p, |c| {
                (*c).rc -= 1;
                if color(c) == 0 {
                    set_color(c, GRAY);
                    s.seen.push(c);
                    s.work.push(c);
                }
            });
        }
        // Scan.
        for i in 0..roots.len {
            s.work.push(roots.get(i));
        }
        while let Some(p) = s.work.pop() {
            if color(p) != GRAY {
                continue;
            }
            if (*p).rc > 0 {
                set_color(p, BLACK);
                s.black.push(p);
                while let Some(b) = s.black.pop() {
                    each_child(b, |c| {
                        (*c).rc += 1;
                        if color(c) != BLACK {
                            set_color(c, BLACK);
                            s.black.push(c);
                        }
                    });
                }
            } else {
                set_color(p, WHITE);
                each_child(p, |c| s.work.push(c));
            }
        }
        // Surviving roots stay roots (with the buffer's reference).
        for i in 0..roots.len {
            let r = roots.get(i);
            if color(r) != WHITE {
                (*r).rc += 1;
                s.roots.push(r);
            }
        }
        s.old.len = 0;
        // Collect white: the references from white to black objects were
        // removed while marking.
        while let Some(x) = s.seen.pop() {
            if color(x) == WHITE {
                let kind = (*x).kind & !COLOR;
                (*x).kind = kind;
                object::release(x, kind);
            } else {
                set_color(x, 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_LOCK as LOCK;
    use crate::object::{CON, cirka_drop};

    unsafe fn con(fields: &[usize], nptr: u8) -> Obj {
        // SAFETY: fresh allocation of a CON with `fields`.
        unsafe {
            let o = crate::alloc::cirka_alloc(8 + 8 * fields.len()) as Obj;
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

    fn frees() -> u64 {
        // SAFETY: the test lock is held.
        unsafe { crate::alloc::stats().frees }
    }

    fn state() -> State {
        State {
            enabled: true,
            roots: PtrVec::new(),
            limit: LIMIT,
            old: PtrVec::new(),
            seen: PtrVec::new(),
            work: PtrVec::new(),
            black: PtrVec::new(),
        }
    }

    /// A garbage cycle a → b → a with a tail b → c is freed; a live cycle
    /// (still referenced) survives intact and stays a root.
    #[test]
    fn trial_deletion() {
        let _g = LOCK.lock().unwrap();
        let mut s = state();
        unsafe {
            let f0 = frees();
            // Garbage: a ⇄ b, b → c.
            let c = con(&[7], 0);
            let b = con(&[0, c as usize], 2);
            let a = con(&[b as usize], 1);
            *word(b, 0) = a as usize;
            (*a).rc += 1; // b → a
            // The program drops its reference to `a` after recording it.
            (*a).rc += 1;
            s.roots.push(a);
            cirka_drop(a);
            // Live: x ⇄ y, the program keeps `x`.
            let y = con(&[0], 1);
            let x = con(&[y as usize], 1);
            *word(y, 0) = x as usize;
            (*x).rc += 1; // y → x
            (*x).rc += 1;
            s.roots.push(x);
            collect(&mut s);
            assert_eq!(frees() - f0, 3, "a, b and c are freed");
            assert_eq!(s.roots.len, 1);
            assert_eq!(s.roots.get(0), x);
            assert_eq!((*x).rc, 3, "program + y + root");
            assert_eq!((*y).rc, 1);
            assert_eq!((*x).kind, CON, "colors are cleared");
            assert_eq!((*y).kind, CON);
            // The program lets go of `x`: the next collection frees x and y.
            cirka_drop(x);
            collect(&mut s);
            assert_eq!(frees() - f0, 5);
            assert_eq!(s.roots.len, 0);
        }
    }
}
