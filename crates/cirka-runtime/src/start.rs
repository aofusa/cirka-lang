//! Start-up, exit, panics, thunk evaluation and partial applications
//! (05-runtime.md §2, §6, §7).

use crate::Global;
use crate::alloc::{self, cirka_alloc};
use crate::io;
use crate::object::*;
use crate::sync::{self, backoff};
use crate::{sys, tls};
use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

struct Startup {
    prog: &'static [u8],
    argc: usize,
    argv: *const *const c_char,
    stack_size: usize,
    /// The signal mask the process started with, for the thread running
    /// the program (the first thread blocks every signal).
    sigmask: sys::sigset_t,
}

/// Written by `main` before the thread running the program starts; read
/// only afterwards.
static STARTUP: Global<Startup> = Global::new(Startup {
    prog: b"cirka",
    argc: 0,
    argv: core::ptr::null(),
    stack_size: 0,
    sigmask: sys::sigset_t { bits: [0; 16] },
});

/// Word 0 of a thunk being evaluated (`BLACKHOLE`) once the evaluating
/// thread is known: this bit and the address of the thread's TLS block,
/// which is unique among the running threads (05-runtime.md §9.5). Code
/// pointers and values never have the top bit set.
const OWNER: usize = 1 << 63;

/// The owner mark of the calling thread.
#[inline(always)]
fn owner_mark() -> usize {
    OWNER | tls::tls() as usize
}

/// Whether a thunk with count `rc` may be evaluated by several threads at
/// once: a shared thunk, or a CAF (or region thunk) once the process is
/// multithreaded.
#[inline(always)]
fn contended(rc: i32) -> bool {
    rc < 0 || (rc == 0 && sync::is_mt())
}

/// Evaluates a thunk to weak head normal form. Consumes the reference `o`
/// and returns an owned reference to the value. Non-thunks are returned
/// unchanged.
///
/// A thunk that other threads may evaluate too is claimed by switching its
/// kind from `THUNK` to `BLACKHOLE` with a compare-and-swap; the others
/// wait for the value (05-runtime.md §9.5). That path is out of line, and
/// the indirection path keeps nothing across a call: an indirection and a
/// thunk of one thread cost what they did before threads existed.
///
/// # Safety
/// `o` must be a live object owned by the caller.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.hot.cirka_force")]
pub unsafe extern "C" fn cirka_force(o: Obj) -> Obj {
    // SAFETY: o is live. The value word of an indirection is written
    // before its kind (Release) and read after it (Acquire).
    unsafe {
        crate::debug::check_live(o, "force");
        match kind_acquire(o) {
            IND => {
                let rc = rc(o);
                if rc == 1 || rc < 0 {
                    return force_ind(o);
                }
                // The indirection survives (or is immortal): no call.
                let r = AtomicUsize::from_ptr(word(o, 0)).load(Ordering::Relaxed) as Obj;
                cirka_dup(r);
                if rc != 0 {
                    (*o).rc = rc - 1;
                }
                r
            }
            // A positive count: only this thread can reach the thunk.
            THUNK if rc(o) > 0 => {
                AtomicU8::from_ptr(&raw mut (*o).kind).store(BLACKHOLE, Ordering::Relaxed);
                evaluate(o)
            }
            THUNK | BLACKHOLE => force_slow(o),
            _ => o,
        }
    }
}

/// `cirka_force` of an indirection that dies or is shared: out of line,
/// so that the other indirections keep nothing across a call.
///
/// # Safety
/// As for `cirka_force`; `o` is an indirection.
#[inline(never)]
unsafe fn force_ind(o: Obj) -> Obj {
    // SAFETY: as for cirka_force.
    unsafe {
        let r = AtomicUsize::from_ptr(word(o, 0)).load(Ordering::Relaxed) as Obj;
        cirka_dup(r);
        cirka_drop(o);
        r
    }
}

/// `cirka_force` of a thunk that other threads may evaluate too, of an
/// immortal thunk (a CAF, a thunk in a region), or of a thunk being
/// evaluated (`BLACKHOLE`).
///
/// # Safety
/// As for `cirka_force`.
#[cold]
#[inline(never)]
unsafe fn force_slow(o: Obj) -> Obj {
    // SAFETY: as for cirka_force.
    unsafe {
        let kind = AtomicU8::from_ptr(&raw mut (*o).kind);
        let w0 = AtomicUsize::from_ptr(word(o, 0));
        let mut spins = 0;
        loop {
            match kind.load(Ordering::Acquire) {
                IND => return cirka_force(o),
                BLACKHOLE => {
                    // The evaluating thread writes its mark just after the
                    // claim: until then (no mark yet) it is another thread.
                    if !sync::is_mt() || w0.load(Ordering::Relaxed) == owner_mark() {
                        return blackhole_loop(o);
                    }
                    backoff(&mut spins);
                }
                THUNK => {
                    if contended(rc(o)) {
                        if kind
                            .compare_exchange(
                                THUNK,
                                BLACKHOLE,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            )
                            .is_err()
                        {
                            continue;
                        }
                    } else {
                        kind.store(BLACKHOLE, Ordering::Relaxed);
                    }
                    return evaluate(o);
                }
                _ => return o,
            }
        }
    }
}

/// The message of a thunk that needs its own value (`<<loop>>`).
static LOOP: crate::exception::StaticBytes<8> = crate::exception::StaticBytes::new(b"<<loop>>");

/// `cirka_force` of a thunk this thread is evaluating: the error
/// `<<loop>>`, reported as an exception (`cirka_force` returns null) when
/// the program unwinds them (05-runtime.md §6.2). The forcing reference is
/// consumed (the evaluation in progress holds another).
///
/// # Safety
/// As for `cirka_force`.
#[cold]
#[inline(never)]
unsafe fn blackhole_loop(o: Obj) -> Obj {
    crate::exception::fail_static(&LOOP);
    // SAFETY: the caller's reference.
    unsafe { cirka_drop(o) };
    core::ptr::null_mut()
}

/// Evaluates the thunk `o` claimed by this thread (now a `BLACKHOLE`) and
/// turns it into an indirection to the value. When the evaluation raised
/// an exception (the entry stub returned null with it pending), the thunk
/// remembers it instead (`raised`) and the result is null.
///
/// # Safety
/// `o` must be a live thunk owned by the caller and claimed by this thread.
#[inline(never)]
unsafe fn evaluate(o: Obj) -> Obj {
    // SAFETY: as for cirka_force; thunk code pointers are generated entry
    // stubs.
    unsafe {
        alloc::stats().forced += 1;
        let w0 = AtomicUsize::from_ptr(word(o, 0));
        let code: extern "C" fn(Obj) -> Obj = core::mem::transmute(w0.load(Ordering::Relaxed));
        w0.store(owner_mark(), Ordering::Relaxed);
        let r = code(o);
        if r.is_null() {
            return raised(o);
        }
        // Drop the captured values and become an indirection.
        let n = (*o).nptr as usize;
        for i in 1..1 + n {
            let c = *word(o, i) as Obj;
            if !c.is_null() {
                cirka_drop(c);
            }
        }
        let rc = rc(o);
        if rc <= 0 {
            return finish_shared(o, r, rc);
        }
        // Still not shared: only this thread can reach the thunk.
        w0.store(r as usize, Ordering::Relaxed);
        (*o).nptr = 1;
        AtomicU8::from_ptr(&raw mut (*o).kind).store(IND, Ordering::Release);
        cirka_dup(r);
        if rc == 1 {
            cirka_free_object(o);
        } else {
            (*o).rc = rc - 1;
        }
        r
    }
}

/// The end of `evaluate` for a thunk whose evaluation raised the pending
/// exception (05-runtime.md §6.4): the captured values the entry stub left
/// (the borrowed ones) are dropped, and the thunk keeps the exception in
/// word 1 with `cirka_thunk_reraise` as its code, which raises it again
/// when the thunk is forced again. Back to `THUNK`, so that the threads
/// waiting for its value evaluate it too and get the same exception. The
/// forcing reference is consumed; the exception stays pending, and the
/// result is null.
///
/// # Safety
/// As for `evaluate`; an exception is pending. The thunk has room for word
/// 1 (the generated code allocates every thunk of a program that unwinds
/// exceptions with at least 24 bytes).
#[cold]
#[inline(never)]
unsafe fn raised(o: Obj) -> Obj {
    // SAFETY: as for evaluate; the pending exception is a live object.
    unsafe {
        let e = crate::exception::cirka_exception_pending();
        let n = (*o).nptr as usize;
        for i in 1..1 + n {
            let c = *word(o, i) as Obj;
            if !c.is_null() {
                cirka_drop(c);
            }
        }
        let rc = rc(o);
        if contended(rc) {
            cirka_mark_shared(e);
        }
        cirka_dup(e);
        *word(o, 1) = e as usize;
        (*o).nptr = 1;
        let reraise: unsafe extern "C" fn(Obj) -> Obj = crate::exception::cirka_thunk_reraise;
        AtomicUsize::from_ptr(word(o, 0)).store(reraise as usize, Ordering::Relaxed);
        AtomicU8::from_ptr(&raw mut (*o).kind).store(THUNK, Ordering::Release);
        if rc != 0 {
            cirka_drop(o);
        }
        core::ptr::null_mut()
    }
}

/// The end of `evaluate` for a thunk that is shared (it may have become
/// shared while it was evaluated: the code spawned a thread that captured
/// it) or immortal (`rc` 0).
///
/// # Safety
/// As for `evaluate`; `r` is the owned value.
#[inline(never)]
unsafe fn finish_shared(o: Obj, r: Obj, rc: i32) -> Obj {
    // SAFETY: as for evaluate.
    unsafe {
        if contended(rc) {
            cirka_mark_shared(r);
        }
        AtomicUsize::from_ptr(word(o, 0)).store(r as usize, Ordering::Relaxed);
        (*o).nptr = 1;
        AtomicU8::from_ptr(&raw mut (*o).kind).store(IND, Ordering::Release);
        cirka_dup(r);
        if rc != 0 {
            cirka_drop(o);
        }
        r
    }
}

/// Builds a partial application of `fun` to `n` arguments (consumes all).
///
/// # Safety
/// `fun` must be a live FUN object; `args` must point to `n` owned objects.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.hot.cirka_mk_pap")]
pub unsafe extern "C" fn cirka_mk_pap(fun: Obj, n: usize, args: *const Obj) -> Obj {
    // SAFETY: fresh allocation of 8 + 8*(1+n) bytes.
    unsafe {
        let p = cirka_alloc(8 + 8 * (1 + n)) as Obj;
        (*p).rc = 1;
        (*p).tag = n as u16;
        (*p).nptr = (1 + n) as u8;
        (*p).kind = PAP;
        *word(p, 0) = fun as usize;
        for i in 0..n {
            *word(p, 1 + i) = *args.add(i) as usize;
        }
        p
    }
}

fn basename(s: &'static [u8]) -> &'static [u8] {
    match s.iter().rposition(|&c| c == b'/') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

/// Prints `<prog>: <msg>` to stderr.
pub fn report_error(msg: &[u8]) {
    // SAFETY: read-only access to start-up data.
    let prog = unsafe { STARTUP.get().prog };
    io::ewrite(prog);
    io::ewrite(b": ");
    io::ewrite(msg);
    io::ewrite(b"\n");
}

/// Runtime error that ends the program (Haskell `error` and pattern match
/// failures in a program without `catch`, internal errors): prints
/// `<prog>: <msg>` to stderr and exits with code 1.
pub fn panic_msg(msg: &[u8]) -> ! {
    report_error(msg);
    exit_after_error()
}

/// Ends the program after its error was reported: exit code 1.
pub fn exit_after_error() -> ! {
    exit_sequence(1)
}

/// # Safety
/// `msg` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_panic(msg: *const u8, len: usize) -> ! {
    // SAFETY: guaranteed by the caller.
    panic_msg(unsafe { core::slice::from_raw_parts(msg, len) })
}

/// The error `o` (a message, borrowed): it ends the program, or, when the
/// program unwinds exceptions, a copy of the reference is the pending
/// exception and this returns (05-runtime.md §6.2).
///
/// # Safety
/// `o` must be a live BYTES object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_panic_bytes(o: Obj) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        if !crate::exception::unwinding() {
            panic_msg(io::bytes_of(o));
        }
        cirka_dup(o);
        crate::exception::pend(o);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn cirka_exit(code: i64) -> ! {
    exit_sequence(code as i32)
}

/// The program name (basename of `argv[0]`) as a fresh BYTES object.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_prog_name() -> Obj {
    // SAFETY: read-only start-up data.
    let p = unsafe { STARTUP.get().prog };
    // SAFETY: `p` is valid for its length.
    unsafe { crate::cirka_bytes_from(p.as_ptr(), p.len()) }
}

/// Number of command line arguments (excluding the program name).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_argc() -> i64 {
    // SAFETY: read-only start-up data.
    unsafe { STARTUP.get().argc.saturating_sub(1) as i64 }
}

/// Argument `i` (0-based, excluding the program name) as a fresh BYTES object.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_arg(i: i64) -> Obj {
    // SAFETY: argv entries are NUL-terminated strings valid for the process.
    unsafe {
        let st = STARTUP.get();
        if i < 0 || i as usize + 1 >= st.argc {
            panic_msg(b"argument index out of range");
        }
        let s = core::ffi::CStr::from_ptr(*st.argv.add(i as usize + 1)).to_bytes();
        crate::cirka_bytes_from(s.as_ptr(), s.len())
    }
}

#[cfg(not(test))]
unsafe extern "C" {
    /// Generated: runs `main` and returns the exit code.
    fn cirka_main() -> i64;
    /// Generated: CAF objects (static thunks) and their number.
    static cirka_caf_roots: [Obj; 0];
    static cirka_caf_count: i64;
}

/// The CAF objects of the program (none in unit tests).
pub(crate) fn caf_roots() -> &'static [Obj] {
    #[cfg(not(test))]
    // SAFETY: the generated table of `cirka_caf_count` objects.
    unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(cirka_caf_roots) as *const Obj,
            cirka_caf_count as usize,
        )
    }
    #[cfg(test)]
    &[]
}

fn exit_sequence(code: i32) -> ! {
    finish(code, true)
}

/// Ends the process like `exit`, without the leak check: an exception
/// nothing caught ended a thread or a callback, and other threads may hold
/// any object (05-runtime.md §6.3).
pub fn exit_unchecked(code: i32) -> ! {
    finish(code, false)
}

/// The exit of the process: flushes the output, reports the statistics
/// and (with `leak_check`) the leaks, as asked.
fn finish(code: i32, leak_check: bool) -> ! {
    // Threads whose result is out (joined, perhaps) finish their exit first:
    // their statistics count, and `running` is exact for the checks below.
    crate::thread::wait_finishing();
    io::flush();
    crate::cycles::finish();
    // SAFETY: statistics and debug state; the leak check runs only when no
    // other thread runs.
    unsafe {
        if matches!(sys::env(c"CIRKA_STATS"), Some(b"1")) {
            let s = alloc::total_stats();
            io::ewrite(b"cirka-stats: allocs=");
            io::ewrite_u64(s.allocs);
            io::ewrite(b" frees=");
            io::ewrite_u64(s.frees);
            io::ewrite(b" max_live=");
            io::ewrite_u64(s.max_live);
            io::ewrite(b" forced=");
            io::ewrite_u64(s.forced);
            io::ewrite(b" array_copies=");
            io::ewrite_u64(s.array_copies);
            io::ewrite(b"\n");
        }
        if leak_check
            && cfg!(feature = "debug-rt")
            && matches!(sys::env(c"CIRKA_LEAK_CHECK"), Some(b"1"))
        {
            // Threads that still run may use any object: no check then.
            let running = crate::thread::running();
            if running == 0 {
                let n = crate::debug::leak_report(caf_roots(), true);
                if n > 0 {
                    io::ewrite(b"cirka: ");
                    io::ewrite_u64(n);
                    io::ewrite(b" objects leaked\n");
                }
            } else {
                io::ewrite(b"cirka: leak check skipped: ");
                io::ewrite_u64(running as u64);
                io::ewrite(b" threads still running\n");
            }
        }
        sys::_exit(code)
    }
}

extern "C" fn on_segv(_sig: c_int, info: *mut sys::siginfo_t, _ctx: *mut c_void) {
    // SAFETY: async-signal-safe reads of the faulting thread's stack bounds
    // (its TLS block) and the fault address.
    unsafe {
        let t = &*tls::tls();
        let addr = (*info).si_addr as usize;
        let lo = t.stack_lo;
        if lo > 0 && addr < lo + (1 << 16) && addr + (16 << 20) > lo {
            io::fatal(b"stack overflow", 2);
        }
        io::fatal(b"segmentation fault", 139);
    }
}

/// Gives the calling thread an alternate signal stack, on which the handler
/// reports a stack overflow. Returns its memory (null if none), which
/// [`remove_alt_stack`] releases.
pub(crate) fn install_alt_stack() -> *mut c_void {
    const ALT: usize = 64 << 10;
    // SAFETY: a fresh allocation registered as this thread's signal stack.
    unsafe {
        let mem = sys::malloc(ALT);
        if mem.is_null() {
            return mem;
        }
        let ss = sys::stack_t {
            ss_sp: mem,
            ss_flags: 0,
            ss_size: ALT,
        };
        sys::sigaltstack(&ss, core::ptr::null_mut());
        mem
    }
}

/// Unregisters and frees the signal stack of an exiting thread.
pub(crate) fn remove_alt_stack(mem: *mut c_void) {
    if mem.is_null() {
        return;
    }
    // SAFETY: the stack registered by `install_alt_stack` on this thread.
    unsafe {
        let ss = sys::stack_t {
            ss_sp: core::ptr::null_mut(),
            ss_flags: sys::SS_DISABLE,
            ss_size: 0,
        };
        sys::sigaltstack(&ss, core::ptr::null_mut());
        sys::free(mem);
    }
}

fn install_segv_handler() {
    install_alt_stack();
    // SAFETY: installing a process-wide handler.
    unsafe {
        let act = sys::sigaction_t {
            sa_sigaction: on_segv as *const () as usize,
            sa_mask: [0; 16],
            sa_flags: sys::SA_SIGINFO | sys::SA_ONSTACK,
            sa_restorer: 0,
        };
        sys::sigaction(sys::SIGSEGV, &act, core::ptr::null_mut());
        sys::sigaction(sys::SIGBUS, &act, core::ptr::null_mut());
    }
}

#[cfg(not(test))]
extern "C" fn main_thread(_arg: *mut c_void) -> *mut c_void {
    let local = 0u8;
    // SAFETY: this thread's TLS block; start-up data written before any
    // other thread exists.
    unsafe {
        let t = &mut *tls::tls();
        t.id = 1;
        t.stack_hi = &local as *const u8 as usize;
        t.stack_lo = t.stack_hi.saturating_sub(STARTUP.get().stack_size);
        sys::pthread_sigmask(
            sys::SIG_SETMASK,
            &STARTUP.get().sigmask,
            core::ptr::null_mut(),
        );
    }
    install_segv_handler();
    // SAFETY: calling the generated entry point.
    let code = unsafe { cirka_main() };
    exit_sequence(code as i32)
}

/// Process entry point: runs `main` on a thread with a large stack
/// (`CIRKA_STACK_SIZE`, default 512M) with a guard page. The first thread
/// only waits for it, with every signal blocked: a signal sent to the
/// process goes to a thread of the program (05-runtime.md §11.3).
///
/// # Safety
/// Called by the C runtime with valid `argc`/`argv`.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    // SAFETY: argv comes from the C runtime.
    unsafe {
        let st = STARTUP.get();
        st.argc = argc as usize;
        st.argv = argv;
        if argc > 0 && !(*argv).is_null() {
            st.prog = basename(core::ffi::CStr::from_ptr(*argv).to_bytes());
        }
        let size = sys::env(c"CIRKA_STACK_SIZE")
            .and_then(sys::parse_size)
            .unwrap_or(512 << 20)
            .max(1 << 20);
        st.stack_size = size;
        let all = sys::sigset_t { bits: [!0; 16] };
        sys::pthread_sigmask(sys::SIG_BLOCK, &all, &mut st.sigmask);
        let mut attr = core::mem::zeroed::<sys::pthread_attr_t>();
        sys::pthread_attr_init(&mut attr);
        sys::pthread_attr_setstacksize(&mut attr, size);
        sys::pthread_attr_setguardsize(&mut attr, 1 << 16);
        let mut t: sys::pthread_t = 0;
        if sys::pthread_create(&mut t, &attr, main_thread, core::ptr::null_mut()) != 0 {
            io::fatal(b"cannot create the main thread", 2);
        }
        sys::pthread_join(t, core::ptr::null_mut());
        0
    }
}
