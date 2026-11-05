//! Start-up, exit, panics, thunk evaluation and partial applications
//! (05-runtime.md §2, §6, §7).

use crate::Global;
use crate::alloc::{STATS, cirka_alloc};
use crate::io;
use crate::object::*;
use crate::sys;
use core::ffi::{c_char, c_int, c_void};

struct Startup {
    prog: &'static [u8],
    argc: usize,
    argv: *const *const c_char,
    stack_top: usize,
    stack_size: usize,
}

static STARTUP: Global<Startup> = Global::new(Startup {
    prog: b"cirka",
    argc: 0,
    argv: core::ptr::null(),
    stack_top: 0,
    stack_size: 0,
});

/// Evaluates a thunk to weak head normal form. Consumes the reference `o`
/// and returns an owned reference to the value. Non-thunks are returned
/// unchanged.
///
/// # Safety
/// `o` must be a live object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_force(o: Obj) -> Obj {
    // SAFETY: o is live; thunk code pointers are generated entry stubs.
    unsafe {
        crate::debug::check_live(o, "force");
        match (*o).kind {
            IND => {
                let r = *word(o, 0) as Obj;
                cirka_dup(r);
                cirka_drop(o);
                r
            }
            BLACKHOLE => panic_msg(b"<<loop>>"),
            THUNK => {
                STATS.get().forced += 1;
                let code: extern "C" fn(Obj) -> Obj = core::mem::transmute(*word(o, 0));
                (*o).kind = BLACKHOLE;
                let r = code(o);
                // Drop the captured values and become an indirection.
                let n = (*o).nptr as usize;
                for i in 1..1 + n {
                    let c = *word(o, i) as Obj;
                    if !c.is_null() {
                        cirka_drop(c);
                    }
                }
                *word(o, 0) = r as usize;
                (*o).nptr = 1;
                (*o).kind = IND;
                cirka_dup(r);
                if (*o).rc != 0 {
                    cirka_drop(o);
                }
                r
            }
            _ => o,
        }
    }
}

/// Builds a partial application of `fun` to `n` arguments (consumes all).
///
/// # Safety
/// `fun` must be a live FUN object; `args` must point to `n` owned objects.
#[unsafe(no_mangle)]
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

/// Runtime error (Haskell `error`, pattern match failure, ...): prints
/// `<prog>: <msg>` to stderr and exits with code 1.
pub fn panic_msg(msg: &[u8]) -> ! {
    // SAFETY: read-only access to start-up data.
    let prog = unsafe { STARTUP.get().prog };
    io::ewrite(prog);
    io::ewrite(b": ");
    io::ewrite(msg);
    io::ewrite(b"\n");
    exit_sequence(1)
}

/// # Safety
/// `msg` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_panic(msg: *const u8, len: usize) -> ! {
    // SAFETY: guaranteed by the caller.
    panic_msg(unsafe { core::slice::from_raw_parts(msg, len) })
}

/// # Safety
/// `o` must be a live BYTES object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_panic_bytes(o: Obj) -> ! {
    // SAFETY: guaranteed by the caller.
    panic_msg(unsafe { io::bytes_of(o) })
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

fn exit_sequence(code: i32) -> ! {
    io::flush();
    crate::cycles::finish();
    // SAFETY: single-threaded at exit; statistics and debug state.
    unsafe {
        if matches!(sys::env(c"CIRKA_STATS"), Some(b"1")) {
            let s = STATS.get();
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
        if cfg!(feature = "debug-rt") && matches!(sys::env(c"CIRKA_LEAK_CHECK"), Some(b"1")) {
            #[cfg(not(test))]
            let roots = core::slice::from_raw_parts(
                core::ptr::addr_of!(cirka_caf_roots) as *const Obj,
                cirka_caf_count as usize,
            );
            #[cfg(test)]
            let roots: &[Obj] = &[];
            let n = crate::debug::leak_report(roots, true);
            if n > 0 {
                io::ewrite(b"cirka: ");
                io::ewrite_u64(n);
                io::ewrite(b" objects leaked\n");
            }
        }
        sys::_exit(code)
    }
}

extern "C" fn on_segv(_sig: c_int, info: *mut sys::siginfo_t, _ctx: *mut c_void) {
    // SAFETY: async-signal-safe reads of start-up data and the fault address.
    unsafe {
        let st = STARTUP.get();
        let addr = (*info).si_addr as usize;
        let lo = st.stack_top.saturating_sub(st.stack_size);
        if st.stack_size > 0 && addr < lo + (1 << 16) && addr + (16 << 20) > lo {
            io::fatal(b"stack overflow", 2);
        }
        io::fatal(b"segmentation fault", 139);
    }
}

fn install_segv_handler() {
    const ALT: usize = 64 << 10;
    // SAFETY: allocating an alternate signal stack and installing a handler.
    unsafe {
        let mem = sys::malloc(ALT);
        if mem.is_null() {
            return;
        }
        let ss = sys::stack_t {
            ss_sp: mem,
            ss_flags: 0,
            ss_size: ALT,
        };
        sys::sigaltstack(&ss, core::ptr::null_mut());
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
    // SAFETY: start-up data written before other threads read it.
    unsafe {
        STARTUP.get().stack_top = &local as *const u8 as usize;
    }
    install_segv_handler();
    // SAFETY: calling the generated entry point.
    let code = unsafe { cirka_main() };
    exit_sequence(code as i32)
}

/// Process entry point: runs `main` on a thread with a large stack
/// (`CIRKA_STACK_SIZE`, default 512M) with a guard page.
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
