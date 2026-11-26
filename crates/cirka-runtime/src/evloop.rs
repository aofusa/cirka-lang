//! Event loops (05-runtime.md §10, FEAT-0046).
//!
//! A loop belongs to the thread that created it and calls callbacks (IO
//! actions) when file descriptors become ready, when timers expire, and
//! when other threads post to its inbox. Callbacks stored in a loop are
//! not marked shared: the loop object must stay on its thread. Other
//! threads reach the loop through a remote (its inbox: a mutex-protected
//! queue and an eventfd), which is reference counted on its own. Blocking
//! work runs on the runtime's thread pool and its result comes back
//! through the inbox.
//!
//! The `cirka_async_*` functions act on the loop running on this thread
//! (the libraries' continuation-passing API, spec-v0.0.5.md §2): a watch
//! may have a timeout, and a watch that cannot be set up calls its
//! callback in the next turn with the error, so that a one-shot
//! continuation always runs exactly once.

use crate::object::{FOREIGN, Obj, cirka_drop, cirka_dup, cirka_mark_shared, word};
use crate::sync::Mutex;
use crate::thread::{cirka_call_1, io_token};
use crate::{Global, alloc, io, sys, tls};
use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Events reported by `cirka_loop_events`.
pub const EV_READ: i64 = 1;
pub const EV_WRITE: i64 = 2;
pub const EV_ERROR: i64 = 4;
/// The timeout of a watch passed first.
pub const EV_TIMEOUT: i64 = 8;

/// Operations that complete at once in one callback before the next ones
/// report `EAGAIN` and wait for the loop (`spend`): one connection whose
/// data keeps coming cannot keep the others waiting.
pub const BUDGET: u32 = 64;

const EINTR: i32 = 4;
const EBADF: i64 = 9;
const ENOENT: i32 = 2;
const EEXIST: i32 = 17;
const EINVAL: i64 = 22;
const EPIPE: i64 = 32;

/// The epoll data of the inbox's eventfd (watches use fd | ident << 32).
const INBOX_DATA: u64 = u64::MAX;

/// Events fetched by one `epoll_wait`.
const BATCH: usize = 256;

fn errno() -> i32 {
    crate::tls::errno()
}

fn oom() -> ! {
    io::fatal(b"out of memory", 2)
}

/// A growable array in `malloc`ed memory.
struct Buf<T: Copy> {
    ptr: *mut T,
    len: usize,
    cap: usize,
}

impl<T: Copy> Buf<T> {
    const fn new() -> Buf<T> {
        Buf {
            ptr: core::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    fn push(&mut self, v: T) {
        if self.len == self.cap {
            let cap = (self.cap * 2).max(8);
            // SAFETY: grows (or creates) the buffer to `cap` elements.
            let p =
                unsafe { sys::realloc(self.ptr as *mut c_void, cap * size_of::<T>()) } as *mut T;
            if p.is_null() {
                oom();
            }
            self.ptr = p;
            self.cap = cap;
        }
        // SAFETY: len < cap.
        unsafe { self.ptr.add(self.len).write(v) };
        self.len += 1;
    }

    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: the element was written.
        Some(unsafe { self.ptr.add(self.len).read() })
    }

    fn get(&self, i: usize) -> T {
        assert!(i < self.len);
        // SAFETY: in bounds.
        unsafe { self.ptr.add(i).read() }
    }

    fn at(&mut self, i: usize) -> &mut T {
        assert!(i < self.len);
        // SAFETY: in bounds; the reference lives as long as the borrow.
        unsafe { &mut *self.ptr.add(i) }
    }

    /// Moves the elements out, leaving the buffer empty.
    fn take(&mut self) -> Buf<T> {
        core::mem::replace(self, Buf::new())
    }

    fn free(&mut self) {
        // An empty buffer that never grew has nothing to free: the loop
        // takes and frees its (usually empty) queues on every turn, without
        // a call into the C library (FEAT-1024).
        if !self.ptr.is_null() {
            // SAFETY: malloc'ed.
            unsafe { sys::free(self.ptr as *mut c_void) };
        }
        *self = Buf::new();
    }
}

/// What a file descriptor waits for, and what the loop knows of it.
///
/// A descriptor stays in the epoll set, edge-triggered, from its first
/// watch until it is closed or unwatched (FEAT-1001): a watch costs no
/// system call when the loop knows the descriptor is not ready, which the
/// operations of `net.rs` tell it (`drain`) when they find nothing to read
/// or no room to write. The kernel reports each change after that.
#[derive(Clone, Copy)]
pub(crate) struct Slot {
    /// The callback (null if the descriptor is not watched).
    cb: Obj,
    /// The registration in the epoll set (the high half of its epoll
    /// data): the serial of the descriptor object for a watch through
    /// `cirka_async_watch`, a new number for every watch by descriptor
    /// number. An event fetched for another registration is ignored.
    ident: u32,
    /// Whether the descriptor is in the epoll set.
    registered: bool,
    /// The events watched for (`EV_READ` | `EV_WRITE`).
    want: u8,
    /// The events the descriptor may be ready for (`EV_READ` |
    /// `EV_WRITE`): set by the events reported, cleared when an operation
    /// found it not ready (`drain`) or by a watch that asked the kernel.
    ready: u8,
    /// An end of stream, error, hang-up or urgent data was reported: a
    /// short read no longer shows that nothing is left to read.
    sticky: bool,
    /// The timer of the watch's timeout (`NO_TIMER` if it has none).
    timer: u32,
}

/// The events of the epoll registration of a watched descriptor: both
/// directions, edge-triggered, with the end of the peer's stream and urgent
/// data (`Slot::sticky`).
const WATCH_MASK: u32 =
    sys::EPOLLIN | sys::EPOLLOUT | sys::EPOLLRDHUP | sys::EPOLLPRI | sys::EPOLLET;

/// Numbers of registrations, shared by the serials of descriptor objects
/// (`net.rs`) and the watches by descriptor number; never 0.
static IDENTS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

/// The `epoll_ctl` calls of watches (tests count them).
#[cfg(test)]
pub(crate) static WATCH_CTLS: AtomicUsize = AtomicUsize::new(0);

/// A new registration number (or serial of a descriptor object).
pub(crate) fn fresh_ident() -> u32 {
    loop {
        let n = IDENTS.fetch_add(1, Ordering::Relaxed);
        if n != 0 {
            return n;
        }
    }
}

/// A timer of the slab.
#[derive(Clone, Copy)]
struct Timer {
    /// The callback; null for the timeout of a watch (whose callback is
    /// in the slot of `fd`).
    cb: Obj,
    epoch: u32,
    next_free: u32,
    /// The descriptor whose watch this timer limits, or -1.
    fd: i32,
    /// The events the callback sees (0, or `-errno` for a watch that
    /// could not be set up).
    ev: i32,
    /// Pending (not fired, cancelled or free).
    used: bool,
}

/// An entry of the timer heap; stale when the slab entry has another
/// generation (the timer fired or was cancelled).
#[derive(Clone, Copy)]
struct Entry {
    deadline: u64,
    seq: u64,
    idx: u32,
    epoch: u32,
}

impl Entry {
    fn before(&self, o: &Entry) -> bool {
        (self.deadline, self.seq) < (o.deadline, o.seq)
    }
}

/// The part of a loop that other threads reach: a queue of actions to run
/// on the loop's thread (posted callbacks, completed blocking work) and
/// the eventfd that wakes the loop. Counted by the loop, its remotes and
/// the blocking jobs in flight.
struct Inbox {
    lock: Mutex,
    refs: AtomicUsize,
    /// Remote objects alive: while there are any, more actions may come
    /// and the loop keeps running.
    remotes: AtomicUsize,
    efd: i32,
    /// Under `lock`: (callback, argument or null), all shared.
    items: Buf<(Obj, Obj)>,
    /// Under `lock`: blocking jobs submitted and not yet completed.
    jobs: usize,
    /// `items.len + jobs`, stored under `lock` whenever either changes, so
    /// that the loop's thread sees an empty inbox without taking the lock
    /// on every turn (FEAT-1024). An action queued after the loop read 0
    /// also wakes the loop through the eventfd, as with the lock.
    pending: AtomicUsize,
    /// Under `lock`: the loop is gone; nothing more is queued.
    closed: bool,
}

impl Inbox {
    /// Records `items.len + jobs` (under `lock`).
    fn note_pending(&self) {
        self.pending
            .store(self.items.len + self.jobs, Ordering::Release);
    }
}

struct Loop {
    owner: u64,
    epfd: i32,
    inbox: *mut Inbox,
    slots: Buf<Slot>,
    watches: usize,
    timers: Buf<Timer>,
    free_timer: u32,
    live_timers: usize,
    heap: Buf<Entry>,
    seq: u64,
    events: i64,
    stop: bool,
    running: bool,
}

const NO_TIMER: u32 = u32::MAX;

/// Nanoseconds of the monotonic clock.
pub(crate) fn now_ns() -> u64 {
    let mut ts = sys::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid out pointer; CLOCK_MONOTONIC = 1.
    unsafe { sys::clock_gettime(1, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// Runs the IO action `cb` (consumed) and drops its result. Every
/// callback starts with a new budget.
///
/// # Safety
/// `cb` must be an owned IO action.
unsafe fn call_action(cb: Obj) {
    // SAFETY: guaranteed by the caller; this thread's block.
    unsafe {
        (*tls::tls()).io_spent = 0;
        let r = tls::call_1()(cb, io_token());
        cirka_drop(r);
    }
}

/// Whether an operation may still complete at once in the running
/// callback: once its budget is spent, operations report `EAGAIN` and
/// wait for the loop instead. Without a running loop there is no budget.
pub(crate) fn budget_left() -> bool {
    // SAFETY: this thread's block.
    unsafe {
        let t = tls::tls();
        (*t).io_spent < BUDGET || (*t).cur_loop.is_null()
    }
}

/// Counts an operation that completed at once.
pub(crate) fn spend() {
    // SAFETY: this thread's block.
    unsafe { (*tls::tls()).io_spent += 1 }
}

/// Runs `cb arg` (both consumed), an IO action.
///
/// # Safety
/// `cb` must be an owned function returning an IO action; `arg` owned.
unsafe fn call_with(cb: Obj, arg: Obj) {
    // SAFETY: guaranteed by the caller.
    unsafe { call_action(tls::call_1()(cb, arg)) }
}

/// The loop state of `o`, which must be a loop owned by this thread.
///
/// # Safety
/// `o` must be a live object.
unsafe fn owned(o: Obj) -> *mut Loop {
    // SAFETY: a FOREIGN object has its pointer in word 0 and its
    // finalizer in word 1.
    unsafe {
        if (*o).kind != FOREIGN || *word(o, 1) != loop_finalize as extern "C" fn(usize) as usize {
            io::fatal(b"not an event loop", 2);
        }
        let l = *word(o, 0) as *mut Loop;
        if (*l).owner != tls::thread_id() {
            io::fatal(
                b"an event loop was used by a thread other than the one that created it (post to a remote instead)",
                2,
            );
        }
        l
    }
}

/// A new FOREIGN object holding `p` with finalizer `fin`.
pub(crate) fn foreign(p: *mut c_void, fin: extern "C" fn(usize)) -> Obj {
    // SAFETY: a fresh object of header + 2 words, no pointer fields.
    unsafe {
        let o = alloc::cirka_alloc(24) as Obj;
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 0;
        (*o).kind = FOREIGN;
        *word(o, 0) = p as usize;
        *word(o, 1) = fin as usize;
        o
    }
}

fn new_inbox() -> *mut Inbox {
    // SAFETY: a fresh allocation, initialized before use; an all-zero
    // pthread mutex is unlocked.
    unsafe {
        let efd = sys::eventfd(0, sys::EFD_NONBLOCK | sys::EFD_CLOEXEC);
        if efd < 0 {
            io::fatal(b"cannot create an eventfd", 2);
        }
        let b = sys::malloc(size_of::<Inbox>()) as *mut Inbox;
        if b.is_null() {
            oom();
        }
        b.write(Inbox {
            lock: Mutex::new(),
            refs: AtomicUsize::new(1),
            remotes: AtomicUsize::new(0),
            efd,
            items: Buf::new(),
            jobs: 0,
            pending: AtomicUsize::new(0),
            closed: false,
        });
        b
    }
}

/// Drops one reference to an inbox; the last one closes its eventfd.
///
/// # Safety
/// `b` must be a live inbox and the caller must own a reference.
unsafe fn inbox_release(b: *mut Inbox) {
    // SAFETY: guaranteed by the caller; the last reference is the only
    // one, and the inbox was closed and emptied by its loop.
    unsafe {
        if (*b).refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            sys::close((*b).efd);
            (*b).items.free();
            sys::free(b as *mut c_void);
        }
    }
}

/// Wakes the loop of inbox `b`.
///
/// # Safety
/// `b` must be a live inbox the caller holds a reference to.
unsafe fn wake(b: *mut Inbox) {
    let one: u64 = 1;
    // SAFETY: the eventfd stays open while the caller's reference lives.
    unsafe { sys::write((*b).efd, &one as *const u64 as *const c_void, 8) };
}

/// Queues `cb` (with `arg`, or null) on the inbox and wakes the loop. With
/// `job`, also counts a blocking job as completed. `false` (nothing
/// queued) if the loop is gone.
///
/// # Safety
/// `b` must be a live inbox the caller holds a reference to; `cb` and
/// `arg` are shared values whose counts the queue takes over.
unsafe fn inbox_push(b: *mut Inbox, cb: Obj, arg: Obj, job: bool) -> bool {
    // SAFETY: the queue fields are guarded by the lock; the eventfd stays
    // open while the caller's reference lives.
    unsafe {
        {
            let _g = (*b).lock.lock();
            if job {
                (*b).jobs -= 1;
            }
            if (*b).closed {
                (*b).note_pending();
                return false;
            }
            (*b).items.push((cb, arg));
            (*b).note_pending();
        }
        wake(b);
        true
    }
}

/// A new event loop owned by the calling thread.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_loop_new() -> Obj {
    // SAFETY: fresh allocations; the eventfd joins the epoll set.
    unsafe {
        let epfd = sys::epoll_create1(sys::EPOLL_CLOEXEC);
        if epfd < 0 {
            io::fatal(b"cannot create an epoll instance", 2);
        }
        let inbox = new_inbox();
        let mut ev = sys::epoll_event {
            events: sys::EPOLLIN,
            data: INBOX_DATA,
        };
        if sys::epoll_ctl(epfd, sys::EPOLL_CTL_ADD, (*inbox).efd, &mut ev) < 0 {
            io::fatal(b"cannot watch the eventfd", 2);
        }
        let l = sys::malloc(size_of::<Loop>()) as *mut Loop;
        if l.is_null() {
            oom();
        }
        l.write(Loop {
            owner: tls::thread_id(),
            epfd,
            inbox,
            slots: Buf::new(),
            watches: 0,
            timers: Buf::new(),
            free_timer: NO_TIMER,
            live_timers: 0,
            heap: Buf::new(),
            seq: 0,
            events: 0,
            stop: false,
            running: false,
        });
        foreign(l as *mut c_void, loop_finalize)
    }
}

/// The finalizer of a loop: drops the callbacks it holds (only on its own
/// thread: they are not shared), closes the inbox and the epoll instance.
extern "C" fn loop_finalize(p: usize) {
    // SAFETY: `p` is the state of a loop object being freed.
    unsafe {
        let l = p as *mut Loop;
        if (*l).owner == tls::thread_id() {
            for i in 0..(*l).slots.len {
                let cb = (*l).slots.get(i).cb;
                if !cb.is_null() {
                    cirka_drop(cb);
                }
            }
            for i in 0..(*l).timers.len {
                let cb = (*l).timers.get(i).cb;
                if !cb.is_null() {
                    cirka_drop(cb);
                }
            }
        }
        (*l).slots.free();
        (*l).timers.free();
        (*l).heap.free();
        sys::close((*l).epfd);
        let b = (*l).inbox;
        let mut items = {
            let _g = (*b).lock.lock();
            (*b).closed = true;
            let items = (*b).items.take();
            (*b).note_pending();
            items
        };
        while let Some((cb, arg)) = items.pop() {
            cirka_drop(cb);
            if !arg.is_null() {
                cirka_drop(arg);
            }
        }
        items.free();
        inbox_release(b);
        sys::free(l as *mut c_void);
    }
}

/// Calls `cb` (borrowed) once when `fd` is ready for `events` (1 read, 2
/// write; an error or hang-up makes it ready too). Replaces an earlier
/// watch of `fd`. 0, or `-errno`.
///
/// # Safety
/// `o` must be a live loop of this thread, `cb` a live IO action.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_watch(o: Obj, fd: i64, events: i64, cb: Obj) -> i64 {
    // SAFETY: the loop's own state, on its thread.
    unsafe { watch(&mut *owned(o), fd, 0, events, -1, cb) }
}

/// `cirka_loop_watch` with a timeout: if `fd` is not ready within `ms`
/// milliseconds (`ms` >= 0), the watch ends and `cb` runs with the event
/// `EV_TIMEOUT`. `ident` is the serial of the descriptor object that owns
/// `fd`, or 0 for a descriptor known by its number only.
///
/// The descriptor is added to the epoll set when the slot has no
/// registration for `ident` (the epoll data then carries a new number for
/// a watch by number), and the kernel reports its state at the next
/// `epoll_wait`. Registered already, it is asked again (`EPOLL_CTL_MOD`,
/// which reports the current state too) only when the slot says it may be
/// ready for `events`: the operation before the watch stopped for its
/// budget, or is not one of those that tell the loop (`drain`). Otherwise
/// the watch makes no system call: the kernel reports the next change.
///
/// The common watch (FEAT-1014: a callback that waits again for its
/// descriptor) is a short path without system calls: a descriptor object
/// registered already, not known to be ready for `events`, no callback or
/// timer in its slot, and no timeout. Everything else takes `watch_slow`.
///
/// # Safety
/// As for `cirka_loop_watch`.
#[inline(always)]
unsafe fn watch(l: &mut Loop, fd: i64, ident: u32, events: i64, ms: i64, cb: Obj) -> i64 {
    // SAFETY: the loop's own state, on its thread; `cb` is live; the slot
    // is within the slots' length (an `fd` below 0 is a large unsigned).
    unsafe {
        let want = (events & (EV_READ | EV_WRITE)) as u8;
        if ms < 0 && ident != 0 && want != 0 && (fd as u64) < l.slots.len as u64 {
            let slot = l.slots.at(fd as usize);
            if slot.registered
                && slot.ident == ident
                && slot.ready & want == 0
                && slot.cb.is_null()
                && slot.timer == NO_TIMER
            {
                slot.want = want;
                cirka_dup(cb);
                slot.cb = cb;
                l.watches += 1;
                return 0;
            }
        }
        watch_slow(l, fd, ident, events, ms, cb)
    }
}

/// Every other watch (see `watch`).
///
/// # Safety
/// As for `cirka_loop_watch`.
#[inline(never)]
unsafe fn watch_slow(l: &mut Loop, fd: i64, ident: u32, events: i64, ms: i64, cb: Obj) -> i64 {
    // SAFETY: the loop's own state, on its thread; `cb` is live.
    unsafe {
        if !(0..=i64::from(i32::MAX)).contains(&fd) {
            return -EBADF;
        }
        let want = (events & (EV_READ | EV_WRITE)) as u8;
        if want == 0 {
            return -EINVAL;
        }
        let fd = fd as usize;
        while l.slots.len <= fd {
            l.slots.push(Slot {
                cb: core::ptr::null_mut(),
                ident: 0,
                registered: false,
                want: 0,
                ready: 0,
                sticky: false,
                timer: NO_TIMER,
            });
        }
        let epfd = l.epfd;
        let slot = l.slots.at(fd);
        let fresh = ident == 0 || !slot.registered || slot.ident != ident;
        if fresh || slot.ready & want != 0 {
            let id = if fresh && ident == 0 {
                fresh_ident()
            } else if fresh {
                ident
            } else {
                slot.ident
            };
            let mut ev = sys::epoll_event {
                events: WATCH_MASK,
                data: (u64::from(id) << 32) | fd as u64,
            };
            let op = if slot.registered {
                sys::EPOLL_CTL_MOD
            } else {
                sys::EPOLL_CTL_ADD
            };
            #[cfg(test)]
            WATCH_CTLS.fetch_add(1, Ordering::Relaxed);
            let mut r = sys::epoll_ctl(epfd, op, fd as i32, &mut ev);
            if r < 0 {
                // The descriptor was closed and its number reused, or was
                // added by an earlier watch that failed half-way.
                let e = errno();
                if op == sys::EPOLL_CTL_MOD && e == ENOENT {
                    r = sys::epoll_ctl(epfd, sys::EPOLL_CTL_ADD, fd as i32, &mut ev);
                } else if op == sys::EPOLL_CTL_ADD && e == EEXIST {
                    r = sys::epoll_ctl(epfd, sys::EPOLL_CTL_MOD, fd as i32, &mut ev);
                }
            }
            if r < 0 {
                slot.registered = false;
                return -i64::from(errno());
            }
            if fresh {
                slot.registered = true;
                slot.ident = id;
                slot.ready = 0;
                slot.sticky = false;
            } else {
                slot.ready &= !want;
            }
        }
        slot.want = want;
        let old = slot.cb;
        let old_timer = slot.timer;
        slot.timer = NO_TIMER;
        cirka_dup(cb);
        slot.cb = cb;
        if old.is_null() {
            l.watches += 1;
        } else {
            cirka_drop(old);
        }
        if old_timer != NO_TIMER {
            free_timer(l, old_timer);
        }
        if ms >= 0 {
            let idx = add_timer(l, ms, core::ptr::null_mut(), fd as i32, 0);
            l.slots.at(fd).timer = idx;
        }
        0
    }
}

/// The slot of `fd` in the loop running on this thread, if it holds the
/// registration `ident` (a descriptor object's serial).
///
/// # Safety
/// On the loop's thread (any thread: null without a running loop).
unsafe fn slot_of(fd: i64, ident: u32) -> *mut Slot {
    // SAFETY: this thread's block; the running loop's own state, the slot
    // within its length.
    unsafe {
        let o = (*tls::tls()).cur_loop;
        if o.is_null() || fd < 0 || ident == 0 {
            return core::ptr::null_mut();
        }
        let l = *word(o, 0) as *mut Loop;
        if fd as usize >= (*l).slots.len {
            return core::ptr::null_mut();
        }
        let slot = (*l).slots.ptr.add(fd as usize);
        if (*slot).registered && (*slot).ident == ident {
            slot
        } else {
            core::ptr::null_mut()
        }
    }
}

/// Whether the loop running on this thread knows that the descriptor `fd`
/// of the object with serial `ident` is not ready for `ev` (`EV_READ` or
/// `EV_WRITE`): the operation would find nothing to read or no room
/// (`EAGAIN`) and may report it without a system call (FEAT-1001).
pub(crate) fn drained(fd: i64, ident: u32, ev: i64) -> bool {
    // SAFETY: the running loop's slot, on its thread.
    unsafe { drained_at(slot(fd, ident), ev) }
}

/// The slot of the descriptor `fd` of the object with serial `ident` in
/// the loop running on this thread, or null (no loop runs, or the
/// descriptor is not registered). An operation keeps it for
/// [`drained_at`] and [`drain_at`] so that it looks the slot up once
/// (FEAT-1016).
#[inline]
pub(crate) fn slot(fd: i64, ident: u32) -> *mut Slot {
    // SAFETY: the running loop's state, on its thread.
    unsafe { slot_of(fd, ident) }
}

/// [`drained`] of the slot `s` (null: not known to be drained).
///
/// # Safety
/// `s` must be null or a slot of the loop running on this thread, from
/// [`slot`] (no watch was added since: that may move the slots).
#[inline]
pub(crate) unsafe fn drained_at(s: *mut Slot, ev: i64) -> bool {
    // SAFETY: guaranteed by the caller.
    unsafe { !s.is_null() && i64::from((*s).ready) & ev == 0 }
}

/// Tells the loop running on this thread that the descriptor `fd` of the
/// object with serial `ident` is closed: the kernel drops the registration
/// with the descriptor, and the slot is free again. (A later object with
/// the same number registers afresh, also if serials wrapped around.)
pub(crate) fn forget(fd: i64, ident: u32) {
    // SAFETY: the running loop's state, on its thread.
    unsafe {
        let s = slot_of(fd, ident);
        if !s.is_null() {
            (*s).registered = false;
        }
    }
}

/// Tells the loop running on this thread that an operation found the
/// descriptor `fd` (serial `ident`) not ready for `ev`: it returned
/// `EAGAIN` (`short` false), or read less than it asked of a stream
/// (`short` true), which shows the same unless an end of stream or urgent
/// data was reported (`Slot::sticky`; a read stops short there). The next
/// edge the kernel reports makes it ready again.
pub(crate) fn drain(fd: i64, ident: u32, ev: i64, short: bool) {
    // SAFETY: the running loop's slot, on its thread.
    unsafe { drain_at(slot(fd, ident), ev, short) }
}

/// [`drain`] of the slot `s` (null: nothing to record).
///
/// # Safety
/// As for [`drained_at`].
#[inline]
pub(crate) unsafe fn drain_at(s: *mut Slot, ev: i64, short: bool) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        if !s.is_null() && !(short && (*s).sticky) {
            (*s).ready &= !(ev as u8);
        }
    }
}

/// Cancels the watch of `fd` (its callback is dropped, not called) and
/// takes `fd` out of the epoll set. 1 if `fd` was watched.
///
/// # Safety
/// `o` must be a live loop of this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_unwatch(o: Obj, fd: i64) -> i64 {
    // SAFETY: the loop's own state, on its thread.
    unsafe {
        let l = &mut *owned(o);
        if fd < 0 || fd as usize >= l.slots.len {
            return 0;
        }
        let epfd = l.epfd;
        let slot = l.slots.at(fd as usize);
        if slot.registered {
            sys::epoll_ctl(epfd, sys::EPOLL_CTL_DEL, fd as i32, core::ptr::null_mut());
            slot.registered = false;
        }
        let cb = slot.cb;
        if cb.is_null() {
            return 0;
        }
        slot.cb = core::ptr::null_mut();
        let timer = slot.timer;
        slot.timer = NO_TIMER;
        if timer != NO_TIMER {
            free_timer(l, timer);
        }
        l.watches -= 1;
        cirka_drop(cb);
        1
    }
}

/// The events of the watch whose callback is running (1 read, 2 write,
/// 4 error or hang-up, which comes with the events watched for); 0
/// elsewhere.
///
/// # Safety
/// `o` must be a live loop of this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_events(o: Obj) -> i64 {
    // SAFETY: the loop's own state, on its thread.
    unsafe { (*owned(o)).events }
}

/// Calls `cb` (borrowed) after `ms` milliseconds. Returns the timer's
/// number (positive) for `cirka_loop_cancel`.
///
/// # Safety
/// `o` must be a live loop of this thread, `cb` a live IO action.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_timer(o: Obj, ms: i64, cb: Obj) -> i64 {
    // SAFETY: the loop's own state, on its thread; `cb` is live.
    unsafe {
        let l = &mut *owned(o);
        cirka_dup(cb);
        let idx = add_timer(l, ms, cb, -1, 0);
        timer_id(l, idx)
    }
}

/// The number of timer `idx` for `cirka_loop_cancel`.
fn timer_id(l: &Loop, idx: u32) -> i64 {
    (i64::from(l.timers.get(idx as usize).epoch) << 32) | (i64::from(idx) + 1)
}

/// Adds a timer that fires after `ms` milliseconds: the callback `cb`
/// (owned; null for the timeout of the watch of `fd`) runs with the
/// events `ev`. Returns its slab entry.
fn add_timer(l: &mut Loop, ms: i64, cb: Obj, fd: i32, ev: i32) -> u32 {
    let deadline = now_ns().saturating_add((ms.max(0) as u64).saturating_mul(1_000_000));
    let idx = if l.free_timer != NO_TIMER {
        let i = l.free_timer;
        l.free_timer = l.timers.get(i as usize).next_free;
        i
    } else {
        l.timers.push(Timer {
            cb: core::ptr::null_mut(),
            epoch: 0,
            next_free: NO_TIMER,
            fd: -1,
            ev: 0,
            used: false,
        });
        (l.timers.len - 1) as u32
    };
    let t = l.timers.at(idx as usize);
    t.epoch = (t.epoch + 1) & 0x7fff_ffff;
    t.cb = cb;
    t.fd = fd;
    t.ev = ev;
    t.used = true;
    let epoch = t.epoch;
    l.seq += 1;
    let seq = l.seq;
    heap_push(
        &mut l.heap,
        Entry {
            deadline,
            seq,
            idx,
            epoch,
        },
    );
    l.live_timers += 1;
    idx
}

/// Cancels a timer: 1 if it had not fired yet (its callback is dropped).
///
/// # Safety
/// `o` must be a live loop of this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_cancel(o: Obj, id: i64) -> i64 {
    // SAFETY: the loop's own state, on its thread.
    unsafe { cancel(&mut *owned(o), id) }
}

/// Cancels the timer numbered `id` (not the timeout of a watch).
///
/// # Safety
/// On the loop's thread.
unsafe fn cancel(l: &mut Loop, id: i64) -> i64 {
    let idx = (id & 0xffff_ffff) - 1;
    let epoch = (id >> 32) as u32;
    if idx < 0 || idx as usize >= l.timers.len {
        return 0;
    }
    let t = l.timers.get(idx as usize);
    if !t.used || t.epoch != epoch || t.fd >= 0 {
        return 0;
    }
    free_timer(l, idx as u32);
    // SAFETY: the timer owned its callback.
    unsafe { cirka_drop(t.cb) };
    1
}

/// Frees slab entry `idx` (its callback was taken or dropped).
fn free_timer(l: &mut Loop, idx: u32) {
    let next = l.free_timer;
    let t = l.timers.at(idx as usize);
    t.cb = core::ptr::null_mut();
    t.used = false;
    t.fd = -1;
    t.next_free = next;
    l.free_timer = idx;
    l.live_timers -= 1;
}

fn heap_push(h: &mut Buf<Entry>, e: Entry) {
    h.push(e);
    let mut i = h.len - 1;
    while i > 0 {
        let p = (i - 1) / 2;
        if !h.get(i).before(&h.get(p)) {
            break;
        }
        let (a, b) = (h.get(i), h.get(p));
        *h.at(i) = b;
        *h.at(p) = a;
        i = p;
    }
}

fn heap_pop(h: &mut Buf<Entry>) -> Option<Entry> {
    if h.len == 0 {
        return None;
    }
    let top = h.get(0);
    let last = h.pop().unwrap();
    if h.len > 0 {
        *h.at(0) = last;
        let mut i = 0;
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            let mut m = i;
            if l < h.len && h.get(l).before(&h.get(m)) {
                m = l;
            }
            if r < h.len && h.get(r).before(&h.get(m)) {
                m = r;
            }
            if m == i {
                break;
            }
            let (a, b) = (h.get(i), h.get(m));
            *h.at(i) = b;
            *h.at(m) = a;
            i = m;
        }
    }
    Some(top)
}

/// Whether heap entry `e` still refers to a pending timer.
fn live(l: &Loop, e: &Entry) -> bool {
    let t = l.timers.get(e.idx as usize);
    t.used && t.epoch == e.epoch
}

/// Runs the timers due at `now`; timers they add wait for the next turn.
/// The timeout of a watch ends the watch and runs its callback with
/// `EV_TIMEOUT`.
///
/// # Safety
/// On the loop's thread.
unsafe fn run_timers(l: *mut Loop, now: u64) {
    // SAFETY: the loop's own state; callbacks may change it between calls,
    // so the callbacks are taken out of the slab and the slots first.
    unsafe {
        let mut due: Buf<(Obj, i64)> = Buf::new();
        while (*l).heap.len > 0 && (*l).heap.get(0).deadline <= now {
            let e = heap_pop(&mut (*l).heap).unwrap();
            if !live(&*l, &e) {
                continue;
            }
            let t = (*l).timers.get(e.idx as usize);
            free_timer(&mut *l, e.idx);
            if t.fd < 0 {
                due.push((t.cb, i64::from(t.ev)));
                continue;
            }
            // The descriptor stays in the epoll set: its events are only
            // noted until the next watch.
            let slot = (*l).slots.at(t.fd as usize);
            slot.timer = NO_TIMER;
            let cb = slot.cb;
            slot.cb = core::ptr::null_mut();
            (*l).watches -= 1;
            due.push((cb, EV_TIMEOUT));
        }
        for i in 0..due.len {
            let (cb, ev) = due.get(i);
            (*l).events = ev;
            call_action(cb);
            (*l).events = 0;
        }
        due.free();
    }
}

/// Milliseconds until the next live timer (stale entries are dropped
/// first); -1 if there is none.
fn timeout_ms(l: &mut Loop) -> i32 {
    while l.heap.len > 0 && !live(l, &l.heap.get(0)) {
        heap_pop(&mut l.heap);
    }
    if l.heap.len == 0 {
        return -1;
    }
    let d = l.heap.get(0).deadline.saturating_sub(now_ns());
    d.div_ceil(1_000_000).min(i32::MAX as u64) as i32
}

/// Runs what other threads queued.
///
/// # Safety
/// On the loop's thread.
unsafe fn run_inbox(l: *mut Loop) {
    // SAFETY: the queue is taken under the lock; its values are shared.
    unsafe {
        let b = (*l).inbox;
        if (*b).pending.load(Ordering::Acquire) == 0 {
            return;
        }
        let mut items = {
            let _g = (*b).lock.lock();
            let items = (*b).items.take();
            (*b).note_pending();
            items
        };
        for i in 0..items.len {
            let (cb, arg) = items.get(i);
            if arg.is_null() {
                call_action(cb);
            } else {
                call_with(cb, arg);
            }
        }
        items.free();
    }
}

/// Whether the inbox has queued actions, and whether blocking jobs are in
/// flight.
///
/// # Safety
/// `b` must be a live inbox.
unsafe fn inbox_state(b: *mut Inbox) -> (bool, bool) {
    // SAFETY: under the lock; an empty inbox without it (`pending`).
    unsafe {
        if (*b).pending.load(Ordering::Acquire) == 0 {
            return (false, false);
        }
        let _g = (*b).lock.lock();
        ((*b).items.len > 0, (*b).jobs > 0)
    }
}

/// Calls the callback of one epoll event.
///
/// # Safety
/// On the loop's thread.
unsafe fn dispatch(l: *mut Loop, ev: sys::epoll_event) {
    // SAFETY: the loop's own state.
    unsafe {
        let data = ev.data;
        if data == INBOX_DATA {
            let mut n: u64 = 0;
            sys::read((*(*l).inbox).efd, &mut n as *mut u64 as *mut c_void, 8);
            return;
        }
        let fd = (data & 0xffff_ffff) as usize;
        let ident = (data >> 32) as u32;
        if fd >= (*l).slots.len {
            return;
        }
        let slot = (*l).slots.at(fd);
        if !slot.registered || slot.ident != ident {
            // An event of an earlier registration (the descriptor was
            // closed or watched again by number since it was fetched).
            return;
        }
        let e = ev.events;
        let mut got = 0u8;
        if e & (sys::EPOLLIN | sys::EPOLLPRI) != 0 {
            got |= EV_READ as u8;
        }
        if e & sys::EPOLLOUT != 0 {
            got |= EV_WRITE as u8;
        }
        let failed = e & (sys::EPOLLERR | sys::EPOLLHUP) != 0;
        if failed {
            // The read or write the callback waits for now fails or sees
            // the end: report it as ready too.
            got |= (EV_READ | EV_WRITE) as u8;
        }
        if failed || e & (sys::EPOLLRDHUP | sys::EPOLLPRI) != 0 {
            slot.sticky = true;
        }
        slot.ready |= got;
        let want = slot.want;
        if slot.cb.is_null() || got & want == 0 {
            // Nothing waits for what is reported: the events are noted
            // (`ready`) for the next operations and watches.
            return;
        }
        let cb = slot.cb;
        slot.cb = core::ptr::null_mut();
        let timer = slot.timer;
        slot.timer = NO_TIMER;
        if timer != NO_TIMER {
            free_timer(&mut *l, timer);
        }
        (*l).watches -= 1;
        let mut seen = i64::from(got & want);
        if failed {
            seen |= EV_ERROR;
        }
        (*l).events = seen;
        call_action(cb);
        (*l).events = 0;
    }
}

/// Runs the loop until nothing is left to wait for (0: no watch, timer,
/// queued action, blocking job or remote) or `cirka_loop_stop` was called
/// (1, at the end of that turn).
///
/// # Safety
/// `o` must be a live loop of this thread.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.hot.cirka_loop_run")]
pub unsafe extern "C" fn cirka_loop_run(o: Obj) -> i64 {
    // SAFETY: the loop's own state, on its thread; the caller's reference
    // keeps the loop alive while callbacks run.
    unsafe {
        let l = owned(o);
        if (*l).running {
            io::fatal(b"cirka_loop_run called from a callback of the same loop", 2);
        }
        (*l).running = true;
        (*l).stop = false;
        let t = tls::tls();
        let outer = (*t).cur_loop;
        (*t).cur_loop = o;
        let mut evs = [sys::epoll_event { events: 0, data: 0 }; BATCH];
        let result = loop {
            run_inbox(l);
            // The clock is read only when there are timers (FEAT-1024).
            if (*l).heap.len > 0 {
                run_timers(l, now_ns());
            }
            if (*l).stop {
                break 1;
            }
            // The remotes first: what a remote posted before it was
            // dropped is then in the queue.
            let remotes = (*(*l).inbox).remotes.load(Ordering::Acquire);
            let (queued, jobs) = inbox_state((*l).inbox);
            if (*l).watches == 0 && (*l).live_timers == 0 && remotes == 0 && !queued && !jobs {
                break 0;
            }
            let timeout = if queued { 0 } else { timeout_ms(&mut *l) };
            let n = sys::epoll_wait((*l).epfd, evs.as_mut_ptr(), BATCH as i32, timeout);
            if n < 0 {
                if errno() == EINTR {
                    continue;
                }
                io::fatal(b"epoll_wait failed", 2);
            }
            for ev in &evs[..n as usize] {
                dispatch(l, *ev);
            }
            if (*l).stop {
                break 1;
            }
        };
        (*l).running = false;
        (*t).cur_loop = outer;
        result
    }
}

/// The loop running on this thread (a new reference): the one whose
/// `cirka_loop_run` called the running callback. Callbacks use it instead
/// of capturing their loop, which would be a cycle (the loop holds its
/// callbacks).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_loop_current() -> Obj {
    // SAFETY: this thread's block; the loop is alive while it runs.
    unsafe {
        let o = (*tls::tls()).cur_loop;
        if o.is_null() {
            io::fatal(b"no event loop is running on this thread", 2);
        }
        cirka_dup(o);
        o
    }
}

/// Makes `cirka_loop_run` return 1 at the end of its current turn.
///
/// # Safety
/// `o` must be a live loop of this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_stop(o: Obj) -> i64 {
    // SAFETY: the loop's own state, on its thread.
    unsafe { (*owned(o)).stop = true };
    0
}

/// A remote of the loop: `cirka_remote_post` on it, from any thread, runs
/// an action on the loop's thread. While a remote is alive the loop does
/// not run out of work: it waits for what the remote may post (as long as
/// a sender of a channel lives).
///
/// # Safety
/// `o` must be a live loop of this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_remote(o: Obj) -> Obj {
    // SAFETY: the loop's own inbox; the remote takes a reference.
    unsafe {
        let b = (*owned(o)).inbox;
        (*b).refs.fetch_add(1, Ordering::Relaxed);
        (*b).remotes.fetch_add(1, Ordering::Relaxed);
        foreign(b as *mut c_void, remote_finalize)
    }
}

extern "C" fn remote_finalize(p: usize) {
    // SAFETY: the inbox reference of a remote object being freed; the
    // last remote wakes the loop, which may have nothing left to wait for.
    unsafe {
        let b = p as *mut Inbox;
        if (*b).remotes.fetch_sub(1, Ordering::AcqRel) == 1 {
            wake(b);
        }
        inbox_release(b);
    }
}

/// Runs the IO action `cb` (borrowed) on the loop's thread. 0, or
/// `-EPIPE` if the loop is gone. Any thread.
///
/// # Safety
/// `rm` must be a live remote, `cb` a live IO action.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_remote_post(rm: Obj, cb: Obj) -> i64 {
    // SAFETY: a FOREIGN remote holds an inbox reference in word 0; `cb`
    // becomes visible to the loop's thread, so it is shared first.
    unsafe {
        if (*rm).kind != FOREIGN || *word(rm, 1) != remote_finalize as extern "C" fn(usize) as usize
        {
            io::fatal(b"not an event loop remote", 2);
        }
        let b = *word(rm, 0) as *mut Inbox;
        cirka_dup(cb);
        cirka_mark_shared(cb);
        if inbox_push(b, cb, core::ptr::null_mut(), false) {
            0
        } else {
            cirka_drop(cb);
            -EPIPE
        }
    }
}

// ---- The loop running on this thread (spec-v0.0.5.md §2) ----

/// The loop running on this thread (borrowed: alive while it runs).
fn current() -> *mut Loop {
    // SAFETY: this thread's block; the loop object is alive while its
    // `cirka_loop_run` runs, and its state is in word 0.
    unsafe {
        let o = (*tls::tls()).cur_loop;
        if o.is_null() {
            io::fatal(
                b"no event loop is running on this thread (run the program in Std.Async.runLoop)",
                2,
            );
        }
        *word(o, 0) as *mut Loop
    }
}

/// Runs `cb` (borrowed) as the first callback of a new loop of this
/// thread, then the loop until nothing is left to wait for (0) or it is
/// stopped (1). The loop is freed afterwards, with what it still holds.
///
/// # Safety
/// `cb` must be a live IO action.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_async_run(cb: Obj) -> i64 {
    // SAFETY: a new loop of this thread, released at the end.
    unsafe {
        let l = cirka_loop_new();
        cirka_loop_timer(l, 0, cb);
        let r = cirka_loop_run(l);
        cirka_drop(l);
        r
    }
}

/// Watches the descriptor of `h` (a descriptor object of `net.rs`) for
/// `events` on the running loop, with a timeout of `ms` milliseconds if
/// `ms` >= 0. `cb` (borrowed) runs exactly once: when the descriptor is
/// ready, when the timeout passes (`EV_TIMEOUT`), or, if the watch cannot
/// be set up, in the next turn with `-errno` as its events.
///
/// # Safety
/// `h` must be a live descriptor object, `cb` a live IO action.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.hot.cirka_async_watch")]
pub unsafe extern "C" fn cirka_async_watch(h: Obj, events: i64, ms: i64, cb: Obj) -> i64 {
    // SAFETY: the running loop's state, on its thread.
    unsafe {
        let l = &mut *current();
        let (fd, ident) = crate::net::handle_ident(h);
        let r = if fd < 0 {
            -EBADF
        } else {
            watch(l, fd, ident, events, ms, cb)
        };
        if r < 0 {
            cirka_dup(cb);
            add_timer(l, 0, cb, -1, r as i32);
        }
        r
    }
}

/// The events of the running callback: 1 read, 2 write, 4 error or
/// hang-up (with the events watched for), 8 timeout, `-errno` when a
/// watch could not be set up; 0 for a timer.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.hot.cirka_async_events")]
pub extern "C" fn cirka_async_events() -> i64 {
    // SAFETY: the running loop's state, on its thread.
    unsafe { (*current()).events }
}

/// `cirka_loop_timer` on the running loop.
///
/// # Safety
/// `cb` must be a live IO action.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_async_timer(ms: i64, cb: Obj) -> i64 {
    // SAFETY: the running loop's state, on its thread; `cb` is live.
    unsafe {
        let l = &mut *current();
        cirka_dup(cb);
        let idx = add_timer(l, ms, cb, -1, 0);
        timer_id(l, idx)
    }
}

/// `cirka_loop_cancel` on the running loop.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_async_cancel(id: i64) -> i64 {
    // SAFETY: the running loop's state, on its thread.
    unsafe { cancel(&mut *current(), id) }
}

/// `cirka_loop_stop` on the running loop.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_async_stop() -> i64 {
    // SAFETY: the running loop's state, on its thread.
    unsafe { (*current()).stop = true };
    0
}

/// `cirka_loop_remote` of the running loop.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_async_remote() -> Obj {
    // SAFETY: the running loop object, on its thread.
    unsafe {
        current();
        cirka_loop_remote((*tls::tls()).cur_loop)
    }
}

/// `cirka_loop_blocking` on the running loop.
///
/// # Safety
/// As for `cirka_loop_blocking`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_async_blocking(work: Obj, done: Obj) -> i64 {
    // SAFETY: the running loop object, on its thread.
    unsafe {
        current();
        cirka_loop_blocking((*tls::tls()).cur_loop, work, done)
    }
}

// ---- The thread pool ----

/// A blocking job: `work` runs on a pool thread, then `done` with its
/// result on the loop's thread.
struct Job {
    work: Obj,
    done: Obj,
    inbox: *mut Inbox,
}

/// Jobs in the order they were submitted.
struct Queue {
    buf: Buf<*mut Job>,
    head: usize,
}

impl Queue {
    fn pop(&mut self) -> Option<*mut Job> {
        if self.head == self.buf.len {
            return None;
        }
        let j = self.buf.get(self.head);
        self.head += 1;
        if self.head == self.buf.len {
            self.buf.len = 0;
            self.head = 0;
        }
        Some(j)
    }
}

static POOL_LOCK: Mutex = Mutex::new();
/// Under POOL_LOCK.
static POOL_COND: Global<sys::pthread_cond_t> = Global::new(sys::pthread_cond_t::INIT);
/// Under POOL_LOCK: queued jobs, and whether the threads were started.
static POOL_JOBS: Global<Queue> = Global::new(Queue {
    buf: Buf::new(),
    head: 0,
});
static POOL_STARTED: Global<bool> = Global::new(false);

/// Runs the IO action `work` (borrowed) on the runtime's thread pool, then
/// `done r` on the loop's thread, where `r` is the result of `work`.
///
/// # Safety
/// `o` must be a live loop of this thread; `work` an IO action and `done`
/// a function returning one.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_loop_blocking(o: Obj, work: Obj, done: Obj) -> i64 {
    // SAFETY: `work` and `done` cross threads, so they are shared; the job
    // holds a reference to the inbox. The pool's state is under POOL_LOCK.
    unsafe {
        let l = owned(o);
        let b = (*l).inbox;
        cirka_dup(work);
        cirka_dup(done);
        cirka_mark_shared(work);
        cirka_mark_shared(done);
        (*b).refs.fetch_add(1, Ordering::Relaxed);
        {
            let _g = (*b).lock.lock();
            (*b).jobs += 1;
            (*b).note_pending();
        }
        let j = sys::malloc(size_of::<Job>()) as *mut Job;
        if j.is_null() {
            oom();
        }
        j.write(Job {
            work,
            done,
            inbox: b,
        });
        crate::thread::job_queued();
        let _g = POOL_LOCK.lock();
        if !*POOL_STARTED.get() {
            start_pool();
            *POOL_STARTED.get() = true;
        }
        POOL_JOBS.get().buf.push(j);
        sys::pthread_cond_signal(POOL_COND.ptr());
        0
    }
}

/// Starts the pool's threads (`CIRKA_POOL_THREADS`, default the number of
/// processors). Under POOL_LOCK.
fn start_pool() {
    if !crate::sync::is_mt() {
        crate::thread::become_mt();
    }
    let n = sys::env(c"CIRKA_POOL_THREADS")
        .and_then(sys::parse_size)
        .unwrap_or(crate::thread::cirka_cpu_count() as usize)
        .clamp(1, 256);
    let stack = crate::thread::thread_stack_size();
    // The workers start with the signals blocked (they inherit the mask of
    // this thread), so that a signal sent to the process goes to a thread
    // of the program, even one that blocked it to receive it
    // (05-runtime.md §11.3).
    let old = crate::os::block_async_signals();
    for _ in 0..n {
        let id = tls::new_id();
        // SAFETY: the worker only reads its number from the argument.
        unsafe { crate::thread::start_detached(worker_main, id as usize as *mut c_void, stack) };
    }
    crate::os::set_signal_mask(&old);
}

extern "C" fn worker_main(arg: *mut c_void) -> *mut c_void {
    let local = 0u8;
    // SAFETY: a new runtime thread; jobs are taken under POOL_LOCK and
    // their values are shared.
    unsafe {
        crate::thread::enter_thread(arg as u64, crate::thread::thread_stack_size(), &local);
        loop {
            let j = {
                let _g = POOL_LOCK.lock();
                loop {
                    if let Some(j) = POOL_JOBS.get().pop() {
                        break j;
                    }
                    sys::pthread_cond_wait(POOL_COND.ptr(), POOL_LOCK.raw());
                }
            };
            let Job { work, done, inbox } = j.read();
            sys::free(j as *mut c_void);
            let r = cirka_call_1(work, io_token());
            cirka_mark_shared(r);
            // The loop may finish as soon as the result is queued, and the
            // process exit then waits for `job_end` (and these statistics).
            crate::thread::job_result_out();
            if !inbox_push(inbox, done, r, true) {
                cirka_drop(done);
                cirka_drop(r);
            }
            inbox_release(inbox);
            alloc::flush_stats();
            crate::thread::job_end();
        }
    }
}

#[cfg(test)]
mod tests {
    //! Callbacks are test "closures" (see `thread::cirka_call_1`): they
    //! find their state in statics, under the test lock.

    use super::*;
    use crate::object::CON;
    use core::sync::atomic::{AtomicI64, AtomicU64};
    use std::sync::Mutex as StdMutex;

    unsafe extern "C" {
        fn pipe(fds: *mut i32) -> i32;
        fn socketpair(domain: i32, ty: i32, protocol: i32, fds: *mut i32) -> i32;
    }

    static LOG: StdMutex<Vec<u8>> = StdMutex::new(Vec::new());
    static OWNER: AtomicU64 = AtomicU64::new(0);
    static FD: AtomicI64 = AtomicI64::new(-1);
    static WFD: AtomicI64 = AtomicI64::new(-1);
    static WORKER: AtomicU64 = AtomicU64::new(0);

    fn log(c: u8) {
        LOG.lock().unwrap().push(c);
    }

    fn take_log() -> Vec<u8> {
        core::mem::take(&mut *LOG.lock().unwrap())
    }

    /// A test closure running `f`.
    fn closure(f: extern "C" fn(Obj) -> Obj) -> Obj {
        let code = (f as *const () as usize).to_ne_bytes();
        // SAFETY: a valid slice.
        unsafe { crate::cirka_bytes_from(code.as_ptr(), 8) }
    }

    /// A constructor with one integer field.
    fn int(v: usize) -> Obj {
        // SAFETY: a fresh object of header + one non-pointer word.
        unsafe {
            let o = alloc::cirka_alloc(16) as Obj;
            (*o).rc = 1;
            (*o).tag = 0;
            (*o).nptr = 0;
            (*o).kind = CON;
            *word(o, 0) = v;
            o
        }
    }

    /// Allocations not yet freed, over all threads.
    fn balance() -> i64 {
        crate::thread::wait_finishing();
        let s = alloc::total_stats();
        s.allocs as i64 - s.frees as i64
    }

    fn pipe_fds() -> (i64, i64) {
        let mut fds = [0i32; 2];
        // SAFETY: valid out array.
        assert_eq!(unsafe { pipe(fds.as_mut_ptr()) }, 0);
        (i64::from(fds[0]), i64::from(fds[1]))
    }

    /// A connected pair of stream sockets (`AF_UNIX`, `SOCK_STREAM`): what
    /// `cirka_sock_recv` reads (with `recv`, FEAT-1013).
    fn stream_pair() -> (i64, i64) {
        let mut fds = [0i32; 2];
        // SAFETY: valid out array.
        assert_eq!(unsafe { socketpair(1, 1, 0, fds.as_mut_ptr()) }, 0);
        (i64::from(fds[0]), i64::from(fds[1]))
    }

    /// Runs `f` with the loop that runs this callback.
    fn with_loop<R>(f: impl FnOnce(Obj) -> R) -> R {
        let l = cirka_loop_current();
        let r = f(l);
        // SAFETY: the reference returned above.
        unsafe { cirka_drop(l) };
        r
    }

    extern "C" fn tick_a(t: Obj) -> Obj {
        log(b'a');
        t
    }

    extern "C" fn tick_b(t: Obj) -> Obj {
        log(b'b');
        t
    }

    extern "C" fn tick_c(t: Obj) -> Obj {
        log(b'c');
        // A timer added by a callback runs in a later turn.
        with_loop(|l| {
            let d = closure(tick_d);
            // SAFETY: the running loop; `d` is borrowed by the call.
            unsafe {
                cirka_loop_timer(l, 0, d);
                cirka_drop(d);
            }
        });
        t
    }

    extern "C" fn tick_d(t: Obj) -> Obj {
        log(b'd');
        t
    }

    extern "C" fn tick_x(t: Obj) -> Obj {
        log(b'x');
        t
    }

    static TOKEN_RC: AtomicI64 = AtomicI64::new(-1);

    extern "C" fn tick_token(t: Obj) -> Obj {
        // SAFETY: the token is a live object.
        TOKEN_RC.store(i64::from(unsafe { (*t).rc }), Ordering::Relaxed);
        let ok = t == crate::thread::io_token();
        log(if ok { b't' } else { b'?' });
        t
    }

    #[test]
    fn callbacks_get_the_static_token() {
        // FEAT-1014: the loop applies callbacks to one static immortal
        // token; running them allocates nothing for it.
        let _g = crate::test_lock();
        let b0 = balance();
        // SAFETY: a loop of this thread and a live closure.
        unsafe {
            let l = cirka_loop_new();
            for _ in 0..3 {
                let c = closure(tick_token);
                cirka_loop_timer(l, 0, c);
                cirka_drop(c);
            }
            let a0 = alloc::stats().allocs;
            assert_eq!(cirka_loop_run(l), 0);
            assert_eq!(
                alloc::stats().allocs,
                a0,
                "no allocation to run the callbacks"
            );
            cirka_drop(l);
        }
        assert_eq!(take_log(), b"ttt");
        assert_eq!(TOKEN_RC.load(Ordering::Relaxed), 0);
        assert_eq!(balance(), b0);
    }

    #[test]
    fn timers_run_in_deadline_order() {
        let _g = crate::test_lock();
        let b0 = balance();
        // SAFETY: a loop of this thread and live closures.
        unsafe {
            let l = cirka_loop_new();
            let (a, b, c, x) = (
                closure(tick_a),
                closure(tick_b),
                closure(tick_c),
                closure(tick_x),
            );
            cirka_loop_timer(l, 30, c);
            cirka_loop_timer(l, 10, a);
            let id = cirka_loop_timer(l, 15, x);
            cirka_loop_timer(l, 20, b);
            assert!(id > 0);
            assert_eq!(cirka_loop_cancel(l, id), 1);
            assert_eq!(cirka_loop_cancel(l, id), 0, "already cancelled");
            assert_eq!(cirka_loop_cancel(l, 12345), 0, "no such timer");
            let start = now_ns();
            assert_eq!(cirka_loop_run(l), 0, "nothing left to wait for");
            assert!(now_ns() - start >= 30_000_000);
            assert_eq!(take_log(), b"abcd");
            // A slab entry is reused under a new number; the old one stays
            // invalid.
            let id2 = cirka_loop_timer(l, 0, a);
            assert_ne!(id2, id);
            assert_eq!(cirka_loop_cancel(l, id), 0);
            assert_eq!(cirka_loop_run(l), 0);
            assert_eq!(take_log(), b"a");
            for f in [a, b, c, x] {
                cirka_drop(f);
            }
            cirka_drop(l);
        }
        assert_eq!(balance(), b0, "every callback is released");
    }

    extern "C" fn stop_it(t: Obj) -> Obj {
        // SAFETY: the running loop.
        with_loop(|l| unsafe { cirka_loop_stop(l) });
        log(b's');
        t
    }

    #[test]
    fn stop_and_drop_a_loop_with_pending_callbacks() {
        let _g = crate::test_lock();
        let b0 = balance();
        let (r, w) = pipe_fds();
        // SAFETY: a loop of this thread and live closures.
        unsafe {
            let l = cirka_loop_new();
            let (s, a) = (closure(stop_it), closure(tick_a));
            cirka_loop_timer(l, 0, s);
            cirka_loop_timer(l, 0, a);
            cirka_loop_timer(l, 60_000, a);
            assert_eq!(cirka_loop_watch(l, r, EV_READ, a), 0);
            // The turn that ran `stop_it` completes, then `run` returns.
            assert_eq!(cirka_loop_run(l), 1);
            assert_eq!(take_log(), b"sa");
            // Dropping the loop drops the callbacks of the timer and the
            // watch without calling them.
            cirka_drop(s);
            cirka_drop(a);
            cirka_drop(l);
            assert!(take_log().is_empty());
            crate::stdio::cirka_fd_close(r);
            crate::stdio::cirka_fd_close(w);
        }
        assert_eq!(balance(), b0);
    }

    extern "C" fn on_readable(t: Obj) -> Obj {
        with_loop(|l| {
            // SAFETY: the running loop; the read result is dropped.
            unsafe {
                let ev = cirka_loop_events(l);
                if ev & EV_READ == 0 {
                    log(b'?');
                }
                if ev & EV_ERROR != 0 {
                    // The writer closed the pipe.
                    log(b'h');
                }
                let fd = FD.load(Ordering::Relaxed);
                let d = crate::stdio::cirka_fd_read(fd, 3);
                let bytes = io::bytes_of(d).to_vec();
                cirka_drop(d);
                if bytes.is_empty() {
                    log(b'.');
                } else {
                    LOG.lock().unwrap().extend_from_slice(&bytes);
                    let cb = closure(on_readable);
                    assert_eq!(cirka_loop_watch(l, fd, EV_READ, cb), 0);
                    cirka_drop(cb);
                }
            }
        });
        t
    }

    extern "C" fn write_and_close(t: Obj) -> Obj {
        let w = WFD.load(Ordering::Relaxed);
        // SAFETY: a fresh string, released after the write.
        unsafe {
            let d = crate::cirka_bytes_from(b"hello".as_ptr(), 5);
            if crate::net::cirka_fd_write_from(w, d, 0) != 5
                || crate::net::cirka_fd_write_from(w, d, 5) != 0
            {
                log(b'!');
            }
            cirka_drop(d);
        }
        crate::stdio::cirka_fd_close(w);
        t
    }

    #[test]
    fn a_watch_runs_once_per_readiness() {
        let _g = crate::test_lock();
        let b0 = balance();
        let (r, w) = pipe_fds();
        FD.store(r, Ordering::Relaxed);
        WFD.store(w, Ordering::Relaxed);
        // SAFETY: a loop of this thread and live closures.
        unsafe {
            let l = cirka_loop_new();
            assert_eq!(cirka_loop_events(l), 0, "no callback is running");
            let (rd, wr) = (closure(on_readable), closure(write_and_close));
            assert_eq!(cirka_loop_watch(l, r, EV_READ, rd), 0);
            cirka_loop_timer(l, 5, wr);
            assert_eq!(cirka_loop_run(l), 0);
            // Three bytes per read, then the end of the input. The writer
            // closed the pipe after writing: every call also sees the
            // hang-up (`h`), reported with the read watched for.
            assert_eq!(String::from_utf8(take_log()).unwrap(), "hhelhloh.");
            cirka_drop(rd);
            cirka_drop(wr);
            cirka_drop(l);
            crate::stdio::cirka_fd_close(r);
        }
        assert_eq!(balance(), b0);
    }

    #[test]
    fn watch_unwatch_and_a_reused_descriptor() {
        let _g = crate::test_lock();
        let b0 = balance();
        // SAFETY: a loop of this thread and live closures.
        unsafe {
            let l = cirka_loop_new();
            let a = closure(tick_a);
            let (r, w) = pipe_fds();
            assert_eq!(cirka_loop_watch(l, r, EV_READ, a), 0);
            assert_eq!(cirka_loop_watch(l, r, EV_READ, a), 0, "replaces the watch");
            assert_eq!(cirka_loop_unwatch(l, r), 1);
            assert_eq!(cirka_loop_unwatch(l, r), 0, "not watched");
            assert_eq!(cirka_loop_watch(l, r, 0, a), -EINVAL);
            assert_eq!(cirka_loop_watch(l, -1, EV_READ, a), -EBADF);
            assert_eq!(cirka_loop_run(l), 0, "nothing to wait for");
            // Closed while watched: the number of the next descriptor is
            // the same, and the new descriptor is watched.
            assert_eq!(cirka_loop_watch(l, r, EV_READ, a), 0);
            crate::stdio::cirka_fd_close(r);
            crate::stdio::cirka_fd_close(w);
            let (r2, w2) = pipe_fds();
            assert_eq!(r2, r);
            assert_eq!(cirka_loop_watch(l, r2, EV_READ, a), 0);
            let d = crate::cirka_bytes_from(b"x".as_ptr(), 1);
            assert_eq!(crate::net::cirka_fd_write_from(w2, d, 0), 1);
            assert_eq!(crate::net::cirka_fd_write_from(w2, d, 2), -EINVAL);
            cirka_drop(d);
            assert_eq!(cirka_loop_run(l), 0);
            assert_eq!(take_log(), b"a", "one call for the new descriptor");
            // A closed descriptor cannot be watched.
            crate::stdio::cirka_fd_close(w2);
            assert_eq!(cirka_loop_watch(l, w2, EV_WRITE, a), -EBADF);
            crate::stdio::cirka_fd_close(r2);
            cirka_drop(a);
            cirka_drop(l);
        }
        assert_eq!(balance(), b0);
    }

    extern "C" fn posted(t: Obj) -> Obj {
        let mine = crate::tls::thread_id() == OWNER.load(Ordering::Relaxed);
        log(if mine { b'p' } else { b'P' });
        t
    }

    #[test]
    fn a_remote_posts_from_another_thread() {
        let _g = crate::test_lock();
        crate::sync::set_mt();
        let b0 = balance();
        OWNER.store(crate::tls::thread_id(), Ordering::Relaxed);
        // SAFETY: a loop of this thread; the remote is shared before it
        // crosses threads.
        unsafe {
            let l = cirka_loop_new();
            let rm = cirka_loop_remote(l);
            cirka_mark_shared(rm);
            let rmu = rm as usize;
            let h = std::thread::spawn(move || {
                // The loop has nothing else to wait for: it waits for the
                // remote, which this thread owns.
                std::thread::sleep(std::time::Duration::from_millis(20));
                let p = closure(posted);
                for _ in 0..3 {
                    assert_eq!(cirka_remote_post(rmu as Obj, p), 0);
                }
                cirka_drop(p);
                // The last remote: the loop runs what was posted and ends.
                cirka_drop(rmu as Obj);
                alloc::thread_exit();
            });
            assert_eq!(cirka_loop_run(l), 0);
            h.join().unwrap();
            assert_eq!(take_log(), b"ppp", "run on the loop's thread");
            // The loop is gone: nothing is queued.
            let rm2 = cirka_loop_remote(l);
            cirka_drop(l);
            let p = closure(posted);
            assert_eq!(cirka_remote_post(rm2, p), -EPIPE);
            cirka_drop(p);
            cirka_drop(rm2);
        }
        assert!(take_log().is_empty());
        assert_eq!(balance(), b0);
    }

    extern "C" fn work(t: Obj) -> Obj {
        // SAFETY: the token is owned.
        unsafe { cirka_drop(t) };
        int(crate::tls::thread_id() as usize)
    }

    extern "C" fn done(r: Obj) -> Obj {
        // SAFETY: the result is owned.
        unsafe {
            WORKER.store(*word(r, 0) as u64, Ordering::Relaxed);
            cirka_drop(r);
        }
        closure(after_done)
    }

    extern "C" fn after_done(t: Obj) -> Obj {
        let mine = crate::tls::thread_id() == OWNER.load(Ordering::Relaxed);
        log(if mine { b'd' } else { b'D' });
        t
    }

    #[test]
    fn blocking_work_runs_on_the_pool() {
        let _g = crate::test_lock();
        let b0 = balance();
        OWNER.store(crate::tls::thread_id(), Ordering::Relaxed);
        // SAFETY: a loop of this thread and live closures.
        unsafe {
            let l = cirka_loop_new();
            let (w, d) = (closure(work), closure(done));
            for _ in 0..4 {
                assert_eq!(cirka_loop_blocking(l, w, d), 0);
            }
            cirka_drop(w);
            cirka_drop(d);
            // The loop waits for the jobs in flight.
            assert_eq!(cirka_loop_run(l), 0);
            assert_eq!(take_log(), b"dddd", "results come back to the loop");
            let wid = WORKER.load(Ordering::Relaxed);
            assert!(wid != 0 && wid != crate::tls::thread_id());
            cirka_drop(l);
        }
        assert_eq!(balance(), b0, "the pool's statistics are in the totals");
    }

    /// The count of queued actions and jobs in flight that the loop reads
    /// without the lock (FEAT-1024) follows every change of the queue.
    #[test]
    fn the_inbox_counts_what_is_pending() {
        let _g = crate::test_lock();
        let b0 = balance();
        OWNER.store(crate::tls::thread_id(), Ordering::Relaxed);
        // SAFETY: a loop of this thread, its inbox and live closures.
        unsafe {
            let l = cirka_loop_new();
            let b = (*owned(l)).inbox;
            let pending = || (*b).pending.load(Ordering::Acquire);
            assert_eq!(pending(), 0);
            assert_eq!(inbox_state(b), (false, false));
            // A job counts until its result has run on the loop (a job
            // that completes turns into a queued action).
            let (w, d) = (closure(work), closure(done));
            assert_eq!(cirka_loop_blocking(l, w, d), 0);
            assert_eq!(cirka_loop_blocking(l, w, d), 0);
            cirka_drop(w);
            cirka_drop(d);
            assert_eq!(pending(), 2);
            assert_eq!(cirka_loop_run(l), 0);
            assert_eq!(take_log(), b"dd");
            assert_eq!(pending(), 0);
            assert_eq!(inbox_state(b), (false, false));
            // A posted action counts until the loop has taken it.
            let rm = cirka_loop_remote(l);
            let p = closure(posted);
            assert_eq!(cirka_remote_post(rm, p), 0);
            cirka_drop(p);
            assert_eq!(pending(), 1);
            assert_eq!(inbox_state(b), (true, false));
            cirka_drop(rm);
            assert_eq!(cirka_loop_run(l), 0);
            assert_eq!(take_log(), b"p");
            assert_eq!(pending(), 0);
            cirka_drop(l);
        }
        assert_eq!(balance(), b0);
    }

    #[test]
    fn the_current_loop_is_the_innermost_running_one() {
        extern "C" fn check(t: Obj) -> Obj {
            let me = OWNER.load(Ordering::Relaxed) as usize;
            log(if with_loop(|l| l as usize) == me {
                b'1'
            } else {
                b'0'
            });
            t
        }
        let _g = crate::test_lock();
        // SAFETY: a loop of this thread and a live closure.
        unsafe {
            let l = cirka_loop_new();
            OWNER.store(l as u64, Ordering::Relaxed);
            let c = closure(check);
            cirka_loop_timer(l, 0, c);
            cirka_loop_run(l);
            assert_eq!(take_log(), b"1");
            assert!((*tls::tls()).cur_loop.is_null(), "none after the run");
            cirka_drop(c);
            cirka_drop(l);
        }
    }

    static EVENTS: StdMutex<Vec<i64>> = StdMutex::new(Vec::new());
    static HANDLE: AtomicU64 = AtomicU64::new(0);

    /// Records the events of the running callback.
    extern "C" fn record(t: Obj) -> Obj {
        EVENTS.lock().unwrap().push(cirka_async_events());
        t
    }

    fn take_events() -> Vec<i64> {
        core::mem::take(&mut *EVENTS.lock().unwrap())
    }

    /// Watches the pipe in `FD` (as a descriptor object) for reading with
    /// a timeout of 20 ms, and the write end in `WFD` with none.
    extern "C" fn watch_with_timeouts(t: Obj) -> Obj {
        // SAFETY: descriptor objects and closures, released after the
        // watches took their references.
        unsafe {
            let r = crate::net::cirka_fd_handle(FD.load(Ordering::Relaxed));
            let w = crate::net::cirka_fd_handle(WFD.load(Ordering::Relaxed));
            let cb = closure(record);
            assert_eq!(cirka_async_watch(r, EV_READ, 20, cb), 0);
            // Writable at once: the timeout of a minute must not keep the
            // loop running.
            assert_eq!(cirka_async_watch(w, EV_WRITE, 60_000, cb), 0);
            cirka_drop(cb);
            // The loop is done with the descriptors before they are
            // closed by the test.
            HANDLE.store(r as u64, Ordering::Relaxed);
            WORKER.store(w as u64, Ordering::Relaxed);
        }
        t
    }

    #[test]
    fn a_watch_with_a_timeout() {
        let _g = crate::test_lock();
        let b0 = balance();
        let (r, w) = pipe_fds();
        FD.store(r, Ordering::Relaxed);
        WFD.store(w, Ordering::Relaxed);
        // SAFETY: a closure run as the first callback of a new loop.
        unsafe {
            let c = closure(watch_with_timeouts);
            let start = now_ns();
            assert_eq!(cirka_async_run(c), 0);
            let took = now_ns() - start;
            assert!(took >= 20_000_000, "the read timed out after 20 ms");
            assert!(took < 10_000_000_000, "the other timeout did not wait");
            assert_eq!(take_events(), vec![EV_WRITE, EV_TIMEOUT]);
            cirka_drop(c);
            // The descriptor objects close the pipe.
            cirka_drop(HANDLE.load(Ordering::Relaxed) as Obj);
            cirka_drop(WORKER.load(Ordering::Relaxed) as Obj);
        }
        assert_eq!(crate::net::cirka_fd_set_nonblocking(r, 1), -EBADF);
        assert_eq!(balance(), b0);
    }

    /// Watches a closed descriptor object, then runs a timer: the watch
    /// fails, and its callback still runs, in the next turn.
    extern "C" fn watch_closed(t: Obj) -> Obj {
        // SAFETY: a descriptor object and closures, released at the end.
        unsafe {
            let (r, w) = pipe_fds();
            crate::stdio::cirka_fd_close(w);
            let h = crate::net::cirka_fd_handle(r);
            assert_eq!(crate::net::cirka_handle_close(h), 0);
            assert_eq!(crate::net::cirka_handle_close(h), -EBADF, "closed once");
            assert_eq!(crate::net::cirka_handle_fd(h), -1);
            let cb = closure(record);
            assert_eq!(cirka_async_watch(h, EV_READ, -1, cb), -EBADF);
            assert!(take_events().is_empty(), "not called at once");
            let a = closure(tick_a);
            let id = cirka_async_timer(0, a);
            assert_eq!(cirka_async_cancel(id), 1);
            assert_eq!(cirka_async_cancel(id), 0);
            cirka_drop(a);
            cirka_drop(cb);
            cirka_drop(h);
        }
        t
    }

    #[test]
    fn a_watch_that_fails_still_calls_back() {
        let _g = crate::test_lock();
        let b0 = balance();
        // SAFETY: a closure run as the first callback of a new loop.
        unsafe {
            let c = closure(watch_closed);
            assert_eq!(cirka_async_run(c), 0);
            assert_eq!(take_events(), vec![-EBADF]);
            assert!(take_log().is_empty(), "the cancelled timer did not run");
            cirka_drop(c);
        }
        assert_eq!(balance(), b0);
    }

    /// Reads the stream socket in `HANDLE` one byte at a time as long as
    /// reads complete at once, logging the bytes, then waits for it.
    extern "C" fn drain_pipe(t: Obj) -> Obj {
        // SAFETY: the descriptor object in `HANDLE`, strings released.
        unsafe {
            let h = HANDLE.load(Ordering::Relaxed) as Obj;
            let mut n = 0;
            loop {
                let d = crate::net::cirka_sock_recv(h, 1);
                let e = crate::stdio::cirka_errno();
                let empty = io::bytes_of(d).is_empty();
                cirka_drop(d);
                if empty {
                    if e == 11 {
                        // EAGAIN: the budget is spent (or the socket empty).
                        log(b'|');
                        let cb = closure(drain_pipe);
                        assert_eq!(cirka_async_watch(h, EV_READ, -1, cb), 0);
                        cirka_drop(cb);
                    }
                    break;
                }
                n += 1;
            }
            EVENTS.lock().unwrap().push(n);
        }
        t
    }

    #[test]
    fn the_budget_lets_other_callbacks_run() {
        let _g = crate::test_lock();
        let b0 = balance();
        let (r, w) = stream_pair();
        // SAFETY: a descriptor object, closures and a string, released.
        unsafe {
            let d = crate::cirka_bytes_from([b'x'; 100].as_ptr(), 100);
            assert_eq!(crate::net::cirka_fd_write_from(w, d, 0), 100);
            cirka_drop(d);
            crate::stdio::cirka_fd_close(w);
            assert_eq!(crate::net::cirka_fd_set_nonblocking(r, 1), 0);
            let h = crate::net::cirka_fd_handle(r);
            HANDLE.store(h as u64, Ordering::Relaxed);
            let c = closure(drain_pipe);
            assert_eq!(cirka_async_run(c), 0);
            // 64 bytes in the first callback, then the rest when the loop
            // calls back; the end of the stream stops the reads.
            assert_eq!(take_events(), vec![64, 36]);
            assert_eq!(take_log(), b"|");
            cirka_drop(c);
            cirka_drop(h);
        }
        assert_eq!(balance(), b0);
    }
}
