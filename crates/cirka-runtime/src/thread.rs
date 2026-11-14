//! Threads (05-runtime.md §9.1): `spawn` runs an IO action on a new OS
//! thread and returns a handle that receives the action's result.
//!
//! The first `spawn` makes the process multithreaded: from then on the
//! values of evaluated CAFs are shared, the standard I/O buffers are
//! locked and thunks shared between threads are evaluated by one thread.

use crate::object::{self, IND, Obj, cirka_drop, cirka_dup, cirka_mark_shared, word};
use crate::sync::{self, TAG_THREAD};
use crate::{alloc, sys, tls};
use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Threads started by `spawn` that have not finished.
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Threads that have stored their result but not yet finished their exit
/// (releasing the handle, adding their statistics to the totals).
static FINISHING: AtomicUsize = AtomicUsize::new(0);

/// Threads started by `spawn` that are still running.
pub fn running() -> usize {
    RUNNING.load(Ordering::Acquire)
}

/// Waits until every thread whose result is out has finished its exit
/// (a few instructions each): at process exit, a joined thread counts as
/// finished.
pub fn wait_finishing() {
    let mut spins = 0;
    while FINISHING.load(Ordering::Acquire) > 0 {
        sync::backoff(&mut spins);
    }
}

#[cfg(not(test))]
unsafe extern "C" {
    /// Generated: applies the closure `f` to `a`, consuming both (the C ABI
    /// entry around `cirka_apply_1`).
    fn cirka_call_1(f: Obj, a: Obj) -> Obj;
}

/// Without generated code (unit tests), a "closure" is a BYTES object
/// whose word 1 holds an `extern "C" fn(Obj) -> Obj`, applied to the token.
#[cfg(test)]
unsafe fn cirka_call_1(f: Obj, a: Obj) -> Obj {
    // SAFETY: the test convention above.
    unsafe {
        let code: extern "C" fn(Obj) -> Obj = core::mem::transmute(*word(f, 1));
        let r = code(a);
        cirka_drop(f);
        r
    }
}

/// What a new thread needs to start.
struct Start {
    f: Obj,
    handle: Obj,
    id: u64,
    stack: usize,
}

/// Makes the process multithreaded: the values of the CAFs evaluated so
/// far become shared (any thread can reach them).
fn become_mt() {
    sync::set_mt();
    // SAFETY: the CAF objects; an evaluated CAF is an IND whose word 0 is
    // its value. No other thread exists yet.
    unsafe {
        for &c in crate::start::caf_roots() {
            if (*c).kind == IND {
                cirka_mark_shared(*word(c, 0) as Obj);
            }
        }
    }
}

/// The token an IO action is applied to (a boxed `Unit#`).
///
/// # Safety
/// Allocates; the caller owns the result.
pub unsafe fn io_token() -> Obj {
    // SAFETY: a fresh object of header + one non-pointer word.
    unsafe {
        let t = alloc::cirka_alloc(16) as Obj;
        (*t).rc = 1;
        (*t).tag = 0;
        (*t).nptr = 0;
        (*t).kind = object::CON;
        *word(t, 0) = 0;
        t
    }
}

/// Runs the IO action `f` (borrowed) on a new thread. Returns its handle,
/// which `cirka_thread_join` waits on.
///
/// # Safety
/// `f` must be a live closure taking the IO token.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_thread_spawn(f: Obj) -> Obj {
    // SAFETY: live objects; the new thread receives counts on `f` and on
    // the handle, both shared before the thread starts.
    unsafe {
        if !sync::is_mt() {
            become_mt();
        }
        cirka_dup(f);
        cirka_mark_shared(f);
        let h = sync::new_sync(TAG_THREAD, core::ptr::null_mut());
        cirka_mark_shared(h);
        cirka_dup(h);
        let stack = sys::env(c"CIRKA_THREAD_STACK_SIZE")
            .and_then(sys::parse_size)
            .unwrap_or(64 << 20)
            .max(1 << 20);
        let st = sys::malloc(core::mem::size_of::<Start>()) as *mut Start;
        if st.is_null() {
            crate::io::fatal(b"out of memory", 2);
        }
        st.write(Start {
            f,
            handle: h,
            id: tls::new_id(),
            stack,
        });
        RUNNING.fetch_add(1, Ordering::AcqRel);
        let mut attr = core::mem::zeroed::<sys::pthread_attr_t>();
        sys::pthread_attr_init(&mut attr);
        sys::pthread_attr_setstacksize(&mut attr, stack);
        sys::pthread_attr_setguardsize(&mut attr, 1 << 16);
        sys::pthread_attr_setdetachstate(&mut attr, sys::PTHREAD_CREATE_DETACHED);
        let mut t: sys::pthread_t = 0;
        let rc = sys::pthread_create(&mut t, &attr, thread_main, st as *mut c_void);
        sys::pthread_attr_destroy(&mut attr);
        if rc != 0 {
            crate::io::fatal(b"cannot create a thread", 2);
        }
        h
    }
}

extern "C" fn thread_main(arg: *mut c_void) -> *mut c_void {
    let local = 0u8;
    // SAFETY: `arg` is the Start written by `cirka_thread_spawn`, owned by
    // this thread; its counts on the closure and the handle are this
    // thread's.
    unsafe {
        let st = (arg as *mut Start).read();
        sys::free(arg);
        let t = &mut *tls::tls();
        t.id = st.id;
        t.stack_hi = &local as *const u8 as usize;
        t.stack_lo = t.stack_hi.saturating_sub(st.stack);
        let alt = crate::start::install_alt_stack();
        let r = cirka_call_1(st.f, io_token());
        cirka_mark_shared(r);
        FINISHING.fetch_add(1, Ordering::AcqRel);
        sync::finish_thread(st.handle, r);
        cirka_drop(st.handle);
        crate::start::remove_alt_stack(alt);
        alloc::thread_exit();
    }
    RUNNING.fetch_sub(1, Ordering::AcqRel);
    FINISHING.fetch_sub(1, Ordering::Release);
    core::ptr::null_mut()
}

/// Waits until the thread of handle `h` (borrowed) finished, and returns
/// its result (a new count; a handle may be joined several times).
///
/// # Safety
/// `h` must be a live thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_thread_join(h: Obj) -> Obj {
    // SAFETY: a live handle (an MVAR object).
    unsafe { sync::cirka_mvar_read(h) }
}

/// The number of the calling thread: 1 for the thread running `main`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_thread_id() -> i64 {
    tls::thread_id() as i64
}

/// Lets other threads run.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_thread_yield() -> i64 {
    // SAFETY: no preconditions.
    unsafe { sys::sched_yield() };
    0
}

/// The number of processors online (at least 1).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_cpu_count() -> i64 {
    // SAFETY: no preconditions.
    (unsafe { sys::sysconf(sys::SC_NPROCESSORS_ONLN) } as i64).max(1)
}
