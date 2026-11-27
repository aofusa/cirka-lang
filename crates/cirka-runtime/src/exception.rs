//! Exceptions (05-runtime.md §6, FEAT-0040, ADR-123..127).
//!
//! A program that catches exceptions (a `catch apply`) unwinds them with
//! the unwinder of the Itanium C++ ABI (libgcc's `_Unwind_RaiseException`):
//! the generated code raises them, its landing pads drop what each frame
//! holds, and `catch apply` stops them. This module has the personality
//! routine the generated code names, the thread's exception state, and the
//! functions the generated code calls to begin, take and report an
//! exception. Rust frames are never unwound (the runtime is built with
//! panic=abort, whose frames abort an unwinding that reaches them): a
//! runtime function reports an exception through the thread's pending slot
//! instead and returns, and the generated code that called it raises it.
//!
//! A program without `catch` does not unwind (it is not told to,
//! `cirka_exception_enable`): an error ends it where it happens.

use crate::object::*;
use crate::{io, tls};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

/// The exception header of the unwinder (`struct _Unwind_Exception`).
#[repr(C, align(16))]
pub struct UnwindException {
    pub class: u64,
    pub cleanup: Option<unsafe extern "C" fn(u32, *mut UnwindException)>,
    pub private: [usize; 2],
}

/// The exception state of a thread (in its TLS block, `tls::Tls::exc`).
/// Landing pads only drop objects, and dropping an object runs no
/// generated code (the finalizers of `FOREIGN` objects are the runtime's),
/// so at most one exception is being raised on a thread at a time: its
/// header and value live here, and raising allocates nothing.
#[repr(C)]
pub struct ExcState {
    /// The header of the exception being raised.
    pub ue: UnwindException,
    /// The value of the exception being raised (owned; null when none).
    pub value: Obj,
    /// The exception a runtime function reports to the generated code that
    /// called it (owned; null when none): §6.2 of 05-runtime.md.
    pub pending: Obj,
}

/// The class of this runtime's exceptions.
const CLASS: u64 = u64::from_be_bytes(*b"CIRKCIRK");

/// Whether the program unwinds exceptions: set by the generated
/// `cirka_main` first, before any other thread exists.
static UNWINDING: AtomicBool = AtomicBool::new(false);

/// Whether the program unwinds exceptions (it has a `catch`): a runtime
/// error is then reported to the generated code (`fail_static`,
/// `fail_msg`); otherwise it ends the program.
#[inline(always)]
pub fn unwinding() -> bool {
    UNWINDING.load(Ordering::Relaxed)
}

/// Called by the generated `cirka_main` of a program that catches
/// exceptions, before anything else (05-runtime.md §6.1).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_exception_enable() {
    UNWINDING.store(true, Ordering::Relaxed);
}

/// This thread's exception state.
#[inline(always)]
fn state() -> *mut ExcState {
    // SAFETY: a field of this thread's block.
    unsafe { &raw mut (*tls::tls()).exc }
}

/// A static byte string object of the runtime (`rc` 0, read-only).
#[repr(C, align(8))]
pub(crate) struct StaticBytes<const N: usize> {
    header: u64,
    len: u64,
    data: [u8; N],
}

impl<const N: usize> StaticBytes<N> {
    pub(crate) const fn new(s: &[u8; N]) -> Self {
        StaticBytes {
            header: (BYTES as u64) << 56,
            len: N as u64,
            data: *s,
        }
    }

    pub(crate) fn obj(&'static self) -> Obj {
        (self as *const Self).cast_mut().cast()
    }
}

/// Makes `v` (owned) the thread's pending exception.
///
/// # Safety
/// `v` must be a live object owned by the caller.
pub(crate) unsafe fn pend(v: Obj) {
    let e = state();
    // SAFETY: this thread's state; a pending exception is raised by the
    // generated code before it calls the runtime again, so there is none
    // (one left behind is dropped).
    unsafe {
        let old = (*e).pending;
        (*e).pending = v;
        if !old.is_null() {
            cirka_drop(old);
        }
    }
}

/// A runtime error with the static message `msg`: the pending exception
/// when the program unwinds exceptions (the caller then returns to the
/// generated code), else the end of the program.
pub(crate) fn fail_static<const N: usize>(msg: &'static StaticBytes<N>) {
    if !unwinding() {
        crate::start::panic_msg(&msg.data);
    }
    // SAFETY: a static object.
    unsafe { pend(msg.obj()) }
}

/// A runtime error with the message `msg`, which becomes a new byte string
/// (as `fail_static`).
pub(crate) fn fail_msg(msg: &[u8]) {
    if !unwinding() {
        crate::start::panic_msg(msg);
    }
    // SAFETY: `msg` is valid for its length; the new object is owned.
    unsafe { pend(crate::cirka_bytes_from(msg.as_ptr(), msg.len())) }
}

/// Does nothing: an exception's header lives in its thread's state.
unsafe extern "C" fn exception_cleanup(_reason: u32, _ue: *mut UnwindException) {}

/// The header to raise `v` (owned) with: the generated `cirka.raise`
/// passes it to `_Unwind_RaiseException`.
///
/// # Safety
/// `v` must be a live object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_exception_begin(v: Obj) -> *mut UnwindException {
    let e = state();
    // SAFETY: this thread's state.
    unsafe {
        if !(*e).value.is_null() {
            // Only possible if dropping an object ran generated code that
            // raised (see `ExcState`).
            io::fatal(b"internal error: exception raised while unwinding", 3);
        }
        (*e).value = v;
        (*e).ue = UnwindException {
            class: CLASS,
            cleanup: Some(exception_cleanup),
            private: [0; 2],
        };
        &raw mut (*e).ue
    }
}

/// The value (owned) of the exception `ue` that a landing pad caught,
/// which ends its unwinding.
///
/// # Safety
/// `ue` must be the header of the exception being raised on this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_exception_take(ue: *mut UnwindException) -> Obj {
    let e = state();
    // SAFETY: this thread's state.
    unsafe {
        debug_assert!(core::ptr::eq(ue, &raw mut (*e).ue));
        let v = (*e).value;
        (*e).value = core::ptr::null_mut();
        v
    }
}

/// The entry stub of a thunk caught the exception `ue` that its code
/// raised: it becomes the pending exception, and the stub returns null to
/// `evaluate` (05-runtime.md §6.4).
///
/// # Safety
/// As for `cirka_exception_take`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_exception_pend(ue: *mut UnwindException) {
    // SAFETY: guaranteed by the caller.
    unsafe { pend(cirka_exception_take(ue)) }
}

/// The pending exception, or null: the generated code asks after calling a
/// runtime function that reports errors through it (`ir::raising_extern`).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_exception_pending() -> Obj {
    // SAFETY: this thread's state.
    unsafe { (*state()).pending }
}

/// Takes the pending exception (owned), to raise it.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_exception_take_pending() -> Obj {
    let e = state();
    // SAFETY: this thread's state.
    unsafe {
        let v = (*e).pending;
        (*e).pending = core::ptr::null_mut();
        v
    }
}

/// Prints the message of an exception nothing caught: `<prog>: <bytes>`
/// for a byte string, else `<prog>: uncaught exception`.
///
/// # Safety
/// `v` must be a live object.
unsafe fn report(v: Obj) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        if !v.is_null() && (*v).kind == BYTES {
            crate::start::report_error(io::bytes_of(v));
        } else {
            crate::start::report_error(b"uncaught exception");
        }
    }
}

/// An exception nothing caught reached `cirka_main` (every frame dropped
/// what it held): prints its message and drops it. `cirka_main` returns
/// the result, 1, as the exit code (05-runtime.md §6.3).
///
/// # Safety
/// As for `cirka_exception_take`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_uncaught(ue: *mut UnwindException) -> i64 {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let v = cirka_exception_take(ue);
        report(v);
        cirka_drop(v);
    }
    1
}

/// An exception nothing caught reached the entry of a thread, a callback
/// of the event loop or a job of the pool (`cirka_call_1`), or a callback
/// from C: prints its message and ends the process with exit code 1. Other
/// threads may still hold objects, so there is no leak check.
///
/// # Safety
/// As for `cirka_exception_take`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_uncaught_exit(ue: *mut UnwindException) -> ! {
    // SAFETY: guaranteed by the caller.
    unsafe { report(cirka_exception_take(ue)) };
    crate::start::exit_unchecked(1)
}

/// `raise x` in a program that does not unwind exceptions: prints the
/// message and ends the program, after dropping `x` (05-runtime.md §6.1).
///
/// # Safety
/// `v` must be a live object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_raise_abort(v: Obj) -> ! {
    // SAFETY: guaranteed by the caller.
    unsafe {
        report(v);
        cirka_drop(v);
    }
    crate::start::exit_after_error()
}

/// `_Unwind_RaiseException` returned: no frame catches the exception, or
/// the stack cannot be unwound. Every entry of the generated code catches
/// (05-runtime.md §6.3), so this is an internal error.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_exception_fatal(_code: i32) -> ! {
    io::fatal(b"internal error: an exception could not be unwound", 3)
}

/// The code of a thunk whose evaluation raised (05-runtime.md §6.4): makes
/// the exception in word 1 pending again, and returns null (to
/// `evaluate`).
///
/// # Safety
/// `t` must be such a thunk, being evaluated by this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_thunk_reraise(t: Obj) -> Obj {
    // SAFETY: word 1 of the thunk holds the exception.
    unsafe {
        let v = *word(t, 1) as Obj;
        cirka_dup(v);
        pend(v);
    }
    core::ptr::null_mut()
}

// ---- the personality routine ----------------------------------------------

type Context = c_void;

unsafe extern "C" {
    fn _Unwind_GetLanguageSpecificData(ctx: *mut Context) -> *const u8;
    fn _Unwind_GetIPInfo(ctx: *mut Context, before: *mut i32) -> usize;
    fn _Unwind_GetRegionStart(ctx: *mut Context) -> usize;
    fn _Unwind_SetGR(ctx: *mut Context, reg: i32, value: usize);
    fn _Unwind_SetIP(ctx: *mut Context, value: usize);
}

/// Actions (`_UA_*`) and results (`_URC_*`) of a personality routine.
const UA_SEARCH_PHASE: i32 = 1;
const UA_FORCE_UNWIND: i32 = 8;
const URC_FATAL_PHASE2_ERROR: i32 = 2;
const URC_FATAL_PHASE1_ERROR: i32 = 3;
const URC_HANDLER_FOUND: i32 = 6;
const URC_INSTALL_CONTEXT: i32 = 7;
const URC_CONTINUE_UNWIND: i32 = 8;

/// A reader of the language-specific data area (`.gcc_except_table`).
struct Reader(*const u8);

impl Reader {
    unsafe fn u8(&mut self) -> u8 {
        // SAFETY: guaranteed by the caller (inside the LSDA).
        unsafe {
            let v = *self.0;
            self.0 = self.0.add(1);
            v
        }
    }

    unsafe fn read<T: Copy>(&mut self) -> T {
        // SAFETY: as for u8.
        unsafe {
            let v = (self.0 as *const T).read_unaligned();
            self.0 = self.0.add(core::mem::size_of::<T>());
            v
        }
    }

    unsafe fn uleb(&mut self) -> u64 {
        let (mut r, mut shift) = (0u64, 0u32);
        loop {
            // SAFETY: as for u8.
            let b = unsafe { self.u8() };
            if shift < 64 {
                r |= ((b & 0x7f) as u64) << shift;
            }
            shift += 7;
            if b & 0x80 == 0 {
                return r;
            }
        }
    }

    unsafe fn sleb(&mut self) -> i64 {
        let (mut r, mut shift) = (0i64, 0u32);
        loop {
            // SAFETY: as for u8.
            let b = unsafe { self.u8() };
            if shift < 64 {
                r |= ((b & 0x7f) as i64) << shift;
            }
            shift += 7;
            if b & 0x80 == 0 {
                if shift < 64 && b & 0x40 != 0 {
                    r |= -1i64 << shift;
                }
                return r;
            }
        }
    }

    /// A value in the pointer encoding `enc` (`DW_EH_PE_*`: absolute,
    /// pc-relative or relative to the function, possibly indirect).
    unsafe fn encoded(&mut self, enc: u8, func: usize) -> Option<usize> {
        if enc == 0xff {
            return None;
        }
        let at = self.0 as usize;
        // SAFETY: as for u8.
        let v = unsafe {
            match enc & 0x0f {
                0x00 => self.read::<usize>(),
                0x01 => self.uleb() as usize,
                0x02 => self.read::<u16>() as usize,
                0x03 => self.read::<u32>() as usize,
                0x04 => self.read::<u64>() as usize,
                0x09 => self.sleb() as usize,
                0x0a => self.read::<i16>() as usize,
                0x0b => self.read::<i32>() as usize,
                0x0c => self.read::<i64>() as usize,
                _ => return None,
            }
        };
        let base = match enc & 0x70 {
            0x00 => 0,
            0x10 => at,
            0x40 => func,
            _ => return None,
        };
        let mut v = v.wrapping_add(base);
        if enc & 0x80 != 0 {
            // SAFETY: an indirect pointer of the LSDA.
            v = unsafe { *(v as *const usize) };
        }
        Some(v)
    }
}

/// What a frame does with an exception at its call site.
enum Action {
    /// Nothing: no landing pad for the call.
    None,
    /// A landing pad that drops what the frame holds and resumes.
    Cleanup(usize),
    /// A landing pad that catches (`catch apply`, an entry).
    Catch(usize),
    /// The call has no entry in the table: it cannot raise.
    Terminate,
}

/// The action of the frame of `ctx` (the LSDA's table of call sites, as
/// LLVM writes it; the generated landing pads either clean up, or catch
/// everything with a `catch ptr null` clause, perhaps also cleaning up
/// after inlining).
///
/// # Safety
/// `ctx` must be an unwinder context.
unsafe fn find_action(ctx: *mut Context) -> Action {
    // SAFETY: the unwinder's context and the LSDA it points to.
    unsafe {
        let lsda = _Unwind_GetLanguageSpecificData(ctx);
        if lsda.is_null() {
            return Action::None;
        }
        let func = _Unwind_GetRegionStart(ctx);
        let mut before = 0;
        let mut ip = _Unwind_GetIPInfo(ctx, &mut before);
        if before == 0 {
            // The return address: the call is the instruction before.
            ip -= 1;
        }
        let mut r = Reader(lsda);
        let lp_enc = r.u8();
        let lpstart = r.encoded(lp_enc, func).unwrap_or(func);
        let tt_enc = r.u8();
        if tt_enc != 0xff {
            r.uleb();
        }
        let cs_enc = r.u8();
        let cs_len = r.uleb() as usize;
        let end = r.0.add(cs_len);
        let actions = end;
        while r.0 < end {
            let start = r.encoded(cs_enc, 0).unwrap_or(0);
            let len = r.encoded(cs_enc, 0).unwrap_or(0);
            let lpad = r.encoded(cs_enc, 0).unwrap_or(0);
            let action = r.uleb();
            if ip < func + start {
                break;
            }
            if ip < func + start + len {
                if lpad == 0 {
                    return Action::None;
                }
                let lpad = lpstart + lpad;
                if action == 0 {
                    return Action::Cleanup(lpad);
                }
                // The first action record: a catch clause (a positive
                // index into the table of types; the generated code has
                // only `catch ptr null`, which catches everything) or a
                // cleanup (0).
                let mut a = Reader(actions.add(action as usize - 1));
                return if a.sleb() > 0 {
                    Action::Catch(lpad)
                } else {
                    Action::Cleanup(lpad)
                };
            }
        }
        Action::Terminate
    }
}

/// The personality routine of the generated code (`personality ptr
/// @cirka_personality`). In the search phase it stops this runtime's
/// exceptions at a landing pad that catches; in the cleanup phase it
/// enters the frame's landing pad with the exception's header (and a
/// selector of 0, which the generated code does not look at). Exceptions
/// of other languages are cleaned up after but never caught.
///
/// # Safety
/// Called by the unwinder only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_personality(
    version: i32,
    actions: i32,
    class: u64,
    ue: *mut UnwindException,
    ctx: *mut Context,
) -> i32 {
    if version != 1 {
        return URC_FATAL_PHASE1_ERROR;
    }
    let ours = class == CLASS;
    // SAFETY: the unwinder's context.
    unsafe {
        let a = find_action(ctx);
        if actions & UA_SEARCH_PHASE != 0 {
            return match a {
                Action::Catch(_) if ours => URC_HANDLER_FOUND,
                Action::Terminate => URC_FATAL_PHASE1_ERROR,
                _ => URC_CONTINUE_UNWIND,
            };
        }
        let lpad = match a {
            Action::None => return URC_CONTINUE_UNWIND,
            Action::Catch(_) if !ours || actions & UA_FORCE_UNWIND != 0 => {
                return URC_CONTINUE_UNWIND;
            }
            Action::Cleanup(l) | Action::Catch(l) => l,
            Action::Terminate => return URC_FATAL_PHASE2_ERROR,
        };
        // rax: the header; rdx: the selector.
        _Unwind_SetGR(ctx, 0, ue as usize);
        _Unwind_SetGR(ctx, 1, 0);
        _Unwind_SetIP(ctx, lpad);
        URC_INSTALL_CONTEXT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::start::cirka_force;

    unsafe fn con(v: usize) -> Obj {
        // SAFETY: a fresh object of one unboxed field.
        unsafe {
            let o = crate::alloc::cirka_alloc(16) as Obj;
            (*o).rc = 1;
            (*o).tag = 0;
            (*o).nptr = 0;
            (*o).kind = CON;
            *word(o, 0) = v;
            o
        }
    }

    /// A thunk of 24 bytes (room for the exception) with `code` and one
    /// captured value (borrowed by the code, so it stays in the thunk).
    unsafe fn thunk(code: unsafe extern "C" fn(Obj) -> Obj, cap: Obj) -> Obj {
        // SAFETY: a fresh object.
        unsafe {
            let t = crate::alloc::cirka_alloc(24) as Obj;
            (*t).rc = 1;
            (*t).tag = 0;
            (*t).nptr = 1;
            (*t).kind = THUNK;
            *word(t, 0) = code as usize;
            *word(t, 1) = cap as usize;
            t
        }
    }

    fn live() -> u64 {
        // SAFETY: this thread's statistics.
        let s = unsafe { crate::alloc::stats() };
        s.allocs - s.frees
    }

    /// The code of a thunk that raises a new object holding its capture's
    /// number plus one, as a generated entry stub does: the exception is
    /// pending and the result null.
    unsafe extern "C" fn raising(t: Obj) -> Obj {
        // SAFETY: the thunk's captured CON.
        unsafe {
            let c = *word(t, 1) as Obj;
            pend(con(*word(c, 0) + 1));
        }
        core::ptr::null_mut()
    }

    /// A thunk whose evaluation raised remembers the exception: forcing it
    /// again raises the same object, its capture is dropped at once, and
    /// nothing is left once the thunk and the exception are dropped
    /// (05-runtime.md §6.4).
    #[test]
    fn a_raised_thunk_raises_again() {
        let _g = crate::test_lock();
        let before = live();
        // SAFETY: objects of this test, each reference consumed once.
        unsafe {
            let cap = con(41);
            let t = thunk(raising, cap);
            cirka_dup(t);
            assert!(cirka_force(t).is_null());
            let e1 = cirka_exception_take_pending();
            assert_eq!(*word(e1, 0), 42);
            assert_eq!((*t).kind, THUNK);
            assert_eq!(((*t).nptr, *word(t, 1)), (1, e1 as usize));
            // The capture is gone: the thunk, the exception and its copy
            // in the thunk are what is left.
            assert_eq!(live(), before + 2);
            assert_eq!(rc(e1), 2);
            assert!(cirka_force(t).is_null());
            let e2 = cirka_exception_take_pending();
            assert_eq!(e1, e2);
            // The thunk died with the last reference.
            assert_eq!(live(), before + 1);
            cirka_drop(e1);
            cirka_drop(e2);
        }
        assert_eq!(live(), before);
    }

    /// A CAF (an immortal thunk) whose evaluation raised before the first
    /// thread keeps an exception that nothing has shared; making the
    /// process multithreaded shares it, as the value of an evaluated CAF
    /// (05-runtime.md §6.4). The state is built by hand: another test may
    /// have made the process multithreaded already, and then the raise
    /// itself shares the exception.
    #[test]
    fn a_raised_caf_shares_its_exception() {
        let _g = crate::test_lock();
        let before = live();
        // SAFETY: objects of this test, each reference consumed once; the
        // CAF is freed by hand (it is immortal).
        unsafe {
            let e = con(7);
            cirka_dup(e);
            let caf = thunk(cirka_thunk_reraise, e);
            (*caf).rc = 0;
            assert!(rc(e) > 0, "not shared yet");
            crate::thread::share_caf(caf);
            assert!(rc(e) < 0, "shared");
            // Forced again, it raises the same (shared) exception.
            assert!(cirka_force(caf).is_null());
            let e2 = cirka_exception_take_pending();
            assert_eq!(e2, e);
            cirka_drop(e2);
            cirka_drop(e);
            cirka_drop(*word(caf, 1) as Obj);
            cirka_free_object(caf);
        }
        assert_eq!(live(), before);
    }

    /// The code of a thunk that forces itself.
    unsafe extern "C" fn self_forcing(t: Obj) -> Obj {
        // SAFETY: the thunk being evaluated, with a new reference.
        unsafe {
            cirka_dup(t);
            let r = cirka_force(t);
            assert!(r.is_null());
        }
        core::ptr::null_mut()
    }

    /// With exceptions, a thunk that needs its own value raises `<<loop>>`
    /// (and remembers it); runtime errors of the byte operations and
    /// `cirka_panic_bytes` are pending exceptions, and the functions
    /// return.
    #[test]
    fn runtime_errors_are_pending_when_unwinding() {
        let _g = crate::test_lock();
        cirka_exception_enable();
        let before = live();
        // SAFETY: objects of this test, each reference consumed once.
        unsafe {
            let cap = con(0);
            let t = thunk(self_forcing, cap);
            assert!(cirka_force(t).is_null());
            let e = cirka_exception_take_pending();
            assert_eq!(io::bytes_of(e), b"<<loop>>");
            assert_eq!(rc(e), 0, "a static message");
            assert_eq!(live(), before);

            let s = crate::cirka_bytes_from(b"hello".as_ptr(), 5);
            assert_eq!(crate::bytes::cirka_bytes_index(s, 108, 0, 9), -1);
            let e = cirka_exception_take_pending();
            assert_eq!(io::bytes_of(e), b"index out of bounds");
            assert_eq!(crate::bytes::cirka_bytes_eq(s, 3, s, 0, 4), 0);
            assert_eq!(
                io::bytes_of(cirka_exception_take_pending()),
                b"index out of bounds"
            );
            assert!(cirka_exception_pending().is_null());

            crate::start::cirka_panic_bytes(s);
            assert_eq!(cirka_exception_take_pending(), s);
            assert_eq!(rc(s), 2, "the exception is a copy of the reference");
            cirka_drop(s);
            cirka_drop(s);
        }
        assert_eq!(live(), before);
        UNWINDING.store(false, Ordering::Relaxed);
    }

    /// The exception being raised lives in the thread's state: `begin`
    /// puts the value there, and `take` (a `catch`) or `pend` (a thunk's
    /// entry stub) ends it.
    #[test]
    fn begin_take_and_pend() {
        let _g = crate::test_lock();
        // SAFETY: objects of this test.
        unsafe {
            let v = con(5);
            let ue = cirka_exception_begin(v);
            assert_eq!((*ue).class, CLASS);
            assert_eq!(cirka_exception_take(ue), v);
            assert!((*state()).value.is_null());
            let ue = cirka_exception_begin(v);
            cirka_exception_pend(ue);
            assert!((*state()).value.is_null());
            assert_eq!(cirka_exception_pending(), v);
            assert_eq!(cirka_exception_take_pending(), v);
            assert!(cirka_exception_pending().is_null());
            cirka_drop(v);
        }
    }

    /// The thread block keeps the offsets the generated code uses, with
    /// the exception state at its end, aligned for the unwinder.
    #[test]
    fn exception_state_in_the_thread_block() {
        use core::mem::offset_of;
        let at = offset_of!(tls::Tls, exc);
        assert_eq!(at % 16, 0);
        assert_eq!(core::mem::size_of::<UnwindException>(), 32);
        assert_eq!(offset_of!(ExcState, value), 32);
        assert_eq!(offset_of!(ExcState, pending), 40);
    }
}
