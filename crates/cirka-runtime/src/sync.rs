//! Synchronization (05-runtime.md §9): the flag of a multithreaded
//! process, the runtime's internal spin lock, and the synchronization
//! objects of kind `MVAR`: an `MVar` (tag 0, empty or one value), a
//! channel (tag 1, an unbounded FIFO) and a thread handle (tag 2, the
//! thread's result once it finished). Word 0 of such an object points to a
//! `malloc`ed [`SyncBox`] holding a pthread mutex, a condition variable and
//! the contents, which the box owns (one count each).
//!
//! Pointer arguments of runtime functions are borrowed: storing a value
//! takes a new count. A value stored into a shared object (negative count)
//! is marked shared first: another thread may take it out.

use crate::object::{self, MVAR, Obj, cirka_drop, cirka_dup, cirka_mark_shared, word};
use crate::sys;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// Set by the first `spawn` and never reset.
static MT: AtomicBool = AtomicBool::new(false);

/// Whether the process may run several threads. Relaxed suffices: only the
/// thread that sets the flag existed before, and `pthread_create` orders
/// the store before everything the new threads do.
#[inline(always)]
pub fn is_mt() -> bool {
    MT.load(Ordering::Relaxed)
}

pub(crate) fn set_mt() {
    MT.store(true, Ordering::Relaxed);
}

/// A lock for the runtime's short critical sections.
pub struct SpinLock(AtomicBool);

pub struct SpinGuard<'a>(&'a SpinLock);

impl SpinLock {
    pub const fn new() -> SpinLock {
        SpinLock(AtomicBool::new(false))
    }

    pub fn lock(&self) -> SpinGuard<'_> {
        let mut spins = 0;
        while self
            .0
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            backoff(&mut spins);
        }
        SpinGuard(self)
    }

    /// Locks only in a multithreaded process (a single thread pays nothing).
    #[inline(always)]
    pub fn lock_if_mt(&self) -> Option<SpinGuard<'_>> {
        if is_mt() { Some(self.lock()) } else { None }
    }
}

impl Default for SpinLock {
    fn default() -> SpinLock {
        SpinLock::new()
    }
}

impl Drop for SpinGuard<'_> {
    fn drop(&mut self) {
        self.0.0.store(false, Ordering::Release);
    }
}

/// A lock for critical sections that may block (the standard I/O buffers
/// around `read` / `write`): a pthread mutex, so waiting threads sleep.
pub struct Mutex(UnsafeCell<sys::pthread_mutex_t>);

// SAFETY: the pthread mutex is shared between threads by design.
unsafe impl Sync for Mutex {}

pub struct MutexGuard<'a>(&'a Mutex);

impl Mutex {
    pub const fn new() -> Mutex {
        Mutex(UnsafeCell::new(sys::pthread_mutex_t::INIT))
    }

    pub fn lock(&self) -> MutexGuard<'_> {
        // SAFETY: a statically initialized mutex.
        unsafe { sys::pthread_mutex_lock(self.0.get()) };
        MutexGuard(self)
    }

    /// Locks only in a multithreaded process (a single thread pays nothing).
    #[inline(always)]
    pub fn lock_if_mt(&self) -> Option<MutexGuard<'_>> {
        if is_mt() { Some(self.lock()) } else { None }
    }

    /// Tries to lock for a while; `None` if another holder keeps it (or
    /// the caller itself holds it: a fatal error reported from inside a
    /// critical section).
    pub fn try_lock(&self) -> Option<MutexGuard<'_>> {
        let mut spins = 0;
        while spins < 256 {
            // SAFETY: a statically initialized mutex.
            if unsafe { sys::pthread_mutex_trylock(self.0.get()) } == 0 {
                return Some(MutexGuard(self));
            }
            backoff(&mut spins);
        }
        None
    }
}

impl Default for Mutex {
    fn default() -> Mutex {
        Mutex::new()
    }
}

impl Drop for MutexGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: the mutex is held by this thread.
        unsafe { sys::pthread_mutex_unlock(self.0.0.get()) };
    }
}

/// Waits a little: spins first, then gives the processor away.
pub fn backoff(spins: &mut u32) {
    if *spins < 64 {
        core::hint::spin_loop();
    } else {
        // SAFETY: no preconditions.
        unsafe { sys::sched_yield() };
    }
    *spins = spins.saturating_add(1);
}

pub const TAG_MVAR: u16 = 0;
pub const TAG_CHAN: u16 = 1;
pub const TAG_THREAD: u16 = 2;

/// The state behind a synchronization object.
#[repr(C)]
pub struct SyncBox {
    mutex: sys::pthread_mutex_t,
    cond: sys::pthread_cond_t,
    /// `MVar` and thread handle: the value, or null when empty.
    slot: Obj,
    /// Channel: a ring buffer of `cap` slots; `len` values from `head`.
    buf: *mut Obj,
    cap: usize,
    head: usize,
    len: usize,
}

/// A new synchronization object of kind `tag` whose slot holds `slot`
/// (owned, or null).
///
/// # Safety
/// `slot` must be null or a live object whose count the box takes over.
pub unsafe fn new_sync(tag: u16, slot: Obj) -> Obj {
    // SAFETY: fresh allocations; the mutex and condition variable are
    // initialized before use.
    unsafe {
        let b = sys::malloc(core::mem::size_of::<SyncBox>()) as *mut SyncBox;
        if b.is_null() {
            crate::io::fatal(b"out of memory", 2);
        }
        core::ptr::write_bytes(b as *mut u8, 0, core::mem::size_of::<SyncBox>());
        sys::pthread_mutex_init(&mut (*b).mutex, core::ptr::null());
        sys::pthread_cond_init(&mut (*b).cond, core::ptr::null());
        (*b).slot = slot;
        let o = crate::alloc::cirka_alloc(16) as Obj;
        (*o).rc = 1;
        (*o).tag = tag;
        (*o).nptr = 0;
        (*o).kind = MVAR;
        *word(o, 0) = b as usize;
        o
    }
}

/// The box's mutex, held until dropped.
struct Locked(*mut SyncBox);

impl Locked {
    /// # Safety
    /// `o` must be a live synchronization object.
    unsafe fn new(o: Obj) -> Locked {
        // SAFETY: word 0 of a live MVAR object is its box.
        unsafe {
            let b = *word(o, 0) as *mut SyncBox;
            sys::pthread_mutex_lock(&mut (*b).mutex);
            Locked(b)
        }
    }

    /// The box; exclusive while the mutex is held (callers keep one such
    /// reference at a time).
    #[allow(clippy::mut_from_ref)]
    fn b(&self) -> &mut SyncBox {
        // SAFETY: the box is live and its mutex is held.
        unsafe { &mut *self.0 }
    }

    /// Waits for a change by another thread. In a process with one thread
    /// nothing can change: GHC's `BlockedIndefinitelyOnMVar`. The mutex is
    /// released first: the exit sequence (the leak check) visits the box.
    fn wait(&self) {
        if !is_mt() {
            // SAFETY: the mutex is held by this thread; `panic_msg` does not
            // return, so the guard never unlocks it again.
            unsafe { sys::pthread_mutex_unlock(&mut (*self.0).mutex) };
            crate::start::panic_msg(b"thread blocked indefinitely in an MVar operation");
        }
        // SAFETY: the mutex is held by this thread.
        unsafe { sys::pthread_cond_wait(&mut (*self.0).cond, &mut (*self.0).mutex) };
    }

    fn wake(&self) {
        // SAFETY: a live condition variable.
        unsafe { sys::pthread_cond_broadcast(&mut (*self.0).cond) };
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        // SAFETY: the mutex is held by this thread.
        unsafe { sys::pthread_mutex_unlock(&mut (*self.0).mutex) };
    }
}

/// Takes a count on `v` for storing it into `o`, marking it shared when
/// `o` is.
///
/// # Safety
/// `o` and `v` must be live objects.
unsafe fn store_count(o: Obj, v: Obj) {
    // SAFETY: live objects.
    unsafe {
        cirka_dup(v);
        if object::rc(o) < 0 {
            cirka_mark_shared(v);
        }
    }
}

/// A new `MVar` holding `v` (borrowed).
///
/// # Safety
/// `v` must be a live object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mvar_new(v: Obj) -> Obj {
    // SAFETY: `v` is live; the box takes the new count.
    unsafe {
        cirka_dup(v);
        new_sync(TAG_MVAR, v)
    }
}

/// A new, empty `MVar`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_mvar_new_empty() -> Obj {
    // SAFETY: an empty slot.
    unsafe { new_sync(TAG_MVAR, core::ptr::null_mut()) }
}

/// Takes the value out of `m`, waiting while it is empty.
///
/// # Safety
/// `m` must be a live `MVar` (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mvar_take(m: Obj) -> Obj {
    // SAFETY: a live MVar.
    let l = unsafe { Locked::new(m) };
    while l.b().slot.is_null() {
        l.wait();
    }
    let v = core::mem::replace(&mut l.b().slot, core::ptr::null_mut());
    l.wake();
    v
}

/// The value of `m` (a new count), waiting while it is empty.
///
/// # Safety
/// `m` must be a live `MVar` or thread handle (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mvar_read(m: Obj) -> Obj {
    // SAFETY: a live synchronization object.
    let l = unsafe { Locked::new(m) };
    while l.b().slot.is_null() {
        l.wait();
    }
    let v = l.b().slot;
    // SAFETY: the box holds a count on `v`.
    unsafe { cirka_dup(v) };
    v
}

/// Puts `v` (borrowed) into `m`, waiting while it is full.
///
/// # Safety
/// `m` must be a live `MVar` and `v` a live object (both borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mvar_put(m: Obj, v: Obj) -> i64 {
    // SAFETY: live objects.
    unsafe { store_count(m, v) };
    // SAFETY: a live MVar.
    let l = unsafe { Locked::new(m) };
    while !l.b().slot.is_null() {
        l.wait();
    }
    l.b().slot = v;
    l.wake();
    0
}

/// Takes the value out of `m`, or returns `d` (borrowed; a new count)
/// when it is empty: the caller tells the two apart by comparing pointers.
///
/// # Safety
/// `m` must be a live `MVar` and `d` a live object (both borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mvar_try_take(m: Obj, d: Obj) -> Obj {
    let v = {
        // SAFETY: a live MVar.
        let l = unsafe { Locked::new(m) };
        let v = core::mem::replace(&mut l.b().slot, core::ptr::null_mut());
        if !v.is_null() {
            l.wake();
        }
        v
    };
    if v.is_null() {
        // SAFETY: `d` is live.
        unsafe { cirka_dup(d) };
        return d;
    }
    v
}

/// Puts `v` (borrowed) into `m` when it is empty: 1, or 0 when it is full.
///
/// # Safety
/// `m` must be a live `MVar` and `v` a live object (both borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mvar_try_put(m: Obj, v: Obj) -> i64 {
    // SAFETY: live objects (marked outside the lock).
    unsafe { store_count(m, v) };
    {
        // SAFETY: a live MVar.
        let l = unsafe { Locked::new(m) };
        if l.b().slot.is_null() {
            l.b().slot = v;
            l.wake();
            return 1;
        }
    }
    // SAFETY: the count taken above, after the lock is released.
    unsafe { cirka_drop(v) };
    0
}

/// A new, empty channel.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_chan_new() -> Obj {
    // SAFETY: an empty slot.
    unsafe { new_sync(TAG_CHAN, core::ptr::null_mut()) }
}

/// Appends `v` (borrowed) to the channel `c` (never waits).
///
/// # Safety
/// `c` must be a live channel and `v` a live object (both borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_chan_send(c: Obj, v: Obj) -> i64 {
    // SAFETY: live objects.
    unsafe { store_count(c, v) };
    // SAFETY: a live channel.
    let l = unsafe { Locked::new(c) };
    let b = l.b();
    if b.len == b.cap {
        grow(b);
    }
    let i = (b.head + b.len) % b.cap;
    // SAFETY: `i < cap` slots of the buffer.
    unsafe { *b.buf.add(i) = v };
    b.len += 1;
    l.wake();
    0
}

/// Doubles the ring buffer, keeping the order of the values.
fn grow(b: &mut SyncBox) {
    let cap = (b.cap * 2).max(8);
    // SAFETY: a fresh buffer of `cap` slots; the old one holds `len` values
    // from `head`, wrapping around.
    unsafe {
        let buf = sys::malloc(cap * 8) as *mut Obj;
        if buf.is_null() {
            crate::io::fatal(b"out of memory", 2);
        }
        for i in 0..b.len {
            *buf.add(i) = *b.buf.add((b.head + i) % b.cap);
        }
        if !b.buf.is_null() {
            sys::free(b.buf as *mut core::ffi::c_void);
        }
        b.buf = buf;
        b.cap = cap;
        b.head = 0;
    }
}

/// Removes the first value of the channel `c` (or returns null when it is
/// empty and `wait` is false).
fn recv(c: Obj, wait: bool) -> Obj {
    // SAFETY: a live channel (callers' contract).
    let l = unsafe { Locked::new(c) };
    while l.b().len == 0 {
        if !wait {
            return core::ptr::null_mut();
        }
        l.wait();
    }
    let b = l.b();
    // SAFETY: `head < cap` holds a value.
    let v = unsafe { *b.buf.add(b.head) };
    b.head = (b.head + 1) % b.cap;
    b.len -= 1;
    v
}

/// Removes the first value of the channel `c`, waiting while it is empty.
///
/// # Safety
/// `c` must be a live channel (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_chan_recv(c: Obj) -> Obj {
    recv(c, true)
}

/// Removes the first value of the channel `c`, or returns `d` (borrowed; a
/// new count) when it is empty.
///
/// # Safety
/// `c` must be a live channel and `d` a live object (both borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_chan_try_recv(c: Obj, d: Obj) -> Obj {
    let v = recv(c, false);
    if v.is_null() {
        // SAFETY: `d` is live.
        unsafe { cirka_dup(d) };
        return d;
    }
    v
}

/// The number of values waiting in the channel `c`.
///
/// # Safety
/// `c` must be a live channel (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_chan_len(c: Obj) -> i64 {
    // SAFETY: a live channel.
    let l = unsafe { Locked::new(c) };
    l.b().len as i64
}

/// Stores the result of a finished thread into its handle (owned `v`,
/// already marked shared).
///
/// # Safety
/// `h` must be a live thread handle and `v` a live object.
pub unsafe fn finish_thread(h: Obj, v: Obj) {
    // SAFETY: a live handle.
    let l = unsafe { Locked::new(h) };
    l.b().slot = v;
    l.wake();
}

/// The values a synchronization object holds, for marking: calls `f` on
/// each under the object's lock.
///
/// # Safety
/// `o` must be a live synchronization object.
pub unsafe fn for_each_value(o: Obj, mut f: impl FnMut(Obj)) {
    // SAFETY: a live synchronization object.
    let l = unsafe { Locked::new(o) };
    let b = l.b();
    if !b.slot.is_null() {
        f(b.slot);
    }
    for i in 0..b.len {
        // SAFETY: `len` values from `head`.
        f(unsafe { *b.buf.add((b.head + i) % b.cap) });
    }
}

/// Releases the state of a dead synchronization object: drops the values
/// it holds and frees the box.
///
/// # Safety
/// `o` must be a dead synchronization object that nothing refers to.
pub unsafe fn release(o: Obj) {
    // SAFETY: nothing else refers to the box any more.
    unsafe {
        let b = *word(o, 0) as *mut SyncBox;
        if !(*b).slot.is_null() {
            cirka_drop((*b).slot);
        }
        for i in 0..(*b).len {
            cirka_drop(*(*b).buf.add(((*b).head + i) % (*b).cap));
        }
        if !(*b).buf.is_null() {
            sys::free((*b).buf as *mut core::ffi::c_void);
        }
        sys::pthread_cond_destroy(&mut (*b).cond);
        sys::pthread_mutex_destroy(&mut (*b).mutex);
        sys::free(b as *mut core::ffi::c_void);
    }
}

/// Whether `o` is a synchronization object.
///
/// # Safety
/// `o` must be a live object.
#[inline]
pub unsafe fn is_sync(o: Obj) -> bool {
    // SAFETY: a live object's header.
    unsafe { (*o).kind == object::MVAR }
}
