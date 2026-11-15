//! C function pointers for closures (FEAT-0047, 08-ffi.md §7.2).
//!
//! A C function pointer is a code address, so a closure handed to C needs
//! code that finds its environment. Writing code at run time would break
//! W^X, so the runtime carries a fixed pool of stubs in `.text`: stub `i`
//! puts its own address in `r10` and jumps to a common entry, which looks
//! up slot `i` of a table, loads the slot's environment into `r10` and
//! jumps to the slot's target. The target is a trampoline generated per
//! callback signature (`@"cb.<name>"`), which takes the environment from
//! `r10` (LLVM's `nest`). Neither the stubs nor the entry touch the
//! argument registers or the stack. A program that never makes a callback
//! loses the stubs and the table to `--gc-sections`.

use crate::object::Obj;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// Callbacks alive at the same time.
pub const SLOTS: usize = 4096;

/// A slot of the table: the environment and the trampoline (16 bytes,
/// as the stubs: slot `i` is at the same offset as stub `i`).
#[repr(C)]
pub struct Slot {
    env: AtomicPtr<u8>,
    target: AtomicPtr<u8>,
}

static TABLE: [Slot; SLOTS] = [const {
    Slot {
        env: AtomicPtr::new(null_mut()),
        target: AtomicPtr::new(null_mut()),
    }
}; SLOTS];

/// The target of a slot taken but not yet filled.
const TAKEN: *mut u8 = core::ptr::dangling_mut::<u8>();

core::arch::global_asm!(
    ".pushsection .text.cirka_callback_stubs,\"ax\",@progbits",
    ".p2align 4",
    ".globl cirka_callback_stubs",
    ".hidden cirka_callback_stubs",
    "cirka_callback_stubs:",
    ".rept {slots}",
    // Its own address (rip after this 7-byte instruction, minus 7).
    "lea r10, [rip - 7]",
    "jmp 2f",
    ".p2align 4, 0xcc",
    ".endr",
    "2:",
    "lea r11, [rip + cirka_callback_stubs]",
    "sub r10, r11",
    "lea r11, [rip + {table}]",
    "add r10, r11",
    "mov r11, qword ptr [r10 + 8]",
    "mov r10, qword ptr [r10]",
    "jmp r11",
    ".popsection",
    slots = const SLOTS,
    table = sym TABLE,
);

unsafe extern "C" {
    static cirka_callback_stubs: [u8; 16 * SLOTS];
}

fn stubs() -> usize {
    core::ptr::addr_of!(cirka_callback_stubs) as usize
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh C function pointer that calls `target` with `env` in `r10`.
/// Takes over the reference to `env` (kept until `cirka_callback_free`).
/// C may call it from any thread of the runtime, so `env` and what it
/// reaches become shared (atomic counts, 05-runtime.md §9.3).
///
/// # Safety
/// `env` must be a live object; `target` a trampoline taking it in `r10`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_callback_new(target: *mut u8, env: Obj) -> i64 {
    // SAFETY: a live object, not yet visible to another thread.
    unsafe { crate::object::cirka_mark_shared(env) };
    let start = NEXT.load(Ordering::Relaxed);
    for k in 0..SLOTS {
        let i = (start + k) % SLOTS;
        let s = &TABLE[i];
        if s.target
            .compare_exchange(null_mut(), TAKEN, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            s.env.store(env as *mut u8, Ordering::Relaxed);
            s.target.store(target, Ordering::Release);
            NEXT.store(i + 1, Ordering::Relaxed);
            return (stubs() + 16 * i) as i64;
        }
    }
    crate::io::fatal(b"too many live C callbacks (at most 4096)", 251)
}

/// Frees the C function pointer `f` made by `cirka_callback_new` and drops
/// its environment. Calling `f` afterwards is the program's error.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_callback_free(f: i64) {
    let off = (f as usize).wrapping_sub(stubs());
    if !off.is_multiple_of(16) || off / 16 >= SLOTS {
        crate::io::fatal(b"cirka_callback_free: not a C callback", 251);
    }
    let s = &TABLE[off / 16];
    let t = s.target.load(Ordering::Acquire);
    if t.is_null() || t == TAKEN {
        crate::io::fatal(b"cirka_callback_free: C callback freed twice", 251);
    }
    let env = s.env.swap(null_mut(), Ordering::Relaxed) as Obj;
    s.target.store(null_mut(), Ordering::Release);
    // SAFETY: the slot owned this reference to a live object.
    unsafe { crate::object::cirka_drop(env) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trampoline of the test: returns its environment (from `r10`)
    /// plus its argument, as LLVM's `nest` would receive it.
    #[unsafe(naked)]
    extern "C" fn add_env(_x: i64) -> i64 {
        core::arch::naked_asm!("lea rax, [rdi + r10]", "ret")
    }

    /// `cirka_callback_new` without marking the fake environment.
    fn new_raw(target: *mut u8, env: usize) -> i64 {
        for (i, s) in TABLE.iter().enumerate() {
            if s.target
                .compare_exchange(null_mut(), TAKEN, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                s.env.store(env as *mut u8, Ordering::Relaxed);
                s.target.store(target, Ordering::Release);
                return (stubs() + 16 * i) as i64;
            }
        }
        panic!("no free slot");
    }

    #[test]
    fn stubs_pass_their_environment() {
        let f = new_raw(add_env as *mut u8, 1000);
        let g = new_raw(add_env as *mut u8, 2000);
        assert_ne!(f, g);
        assert_eq!(f % 16, 0);
        // SAFETY: the stubs jump to `add_env`, a C function of this type.
        let (call_f, call_g) = unsafe {
            (
                core::mem::transmute::<usize, extern "C" fn(i64) -> i64>(f as usize),
                core::mem::transmute::<usize, extern "C" fn(i64) -> i64>(g as usize),
            )
        };
        assert_eq!(call_f(1), 1001);
        assert_eq!(call_g(2), 2002);
        // Free without dropping the fake environments.
        for p in [f, g] {
            let s = &TABLE[(p as usize - stubs()) / 16];
            s.env.store(null_mut(), Ordering::Relaxed);
            s.target.store(null_mut(), Ordering::Release);
        }
    }

    #[test]
    fn every_stub_is_16_bytes() {
        // The stubs near the common entry jump with a shorter instruction;
        // each still starts 16 bytes after the previous one.
        for i in [0, 1, SLOTS - 9, SLOTS - 1] {
            let s = &TABLE[i];
            if s.target
                .compare_exchange(null_mut(), TAKEN, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }
            s.env.store(i as *mut u8, Ordering::Relaxed);
            s.target.store(add_env as *mut u8, Ordering::Release);
            // SAFETY: stub `i` jumps to `add_env` with its environment.
            let call = unsafe {
                core::mem::transmute::<usize, extern "C" fn(i64) -> i64>(stubs() + 16 * i)
            };
            assert_eq!(call(5), i as i64 + 5);
            s.env.store(null_mut(), Ordering::Relaxed);
            s.target.store(null_mut(), Ordering::Release);
        }
    }

    #[test]
    fn freeing_drops_the_environment_once() {
        // SAFETY: a fresh object of no fields, filled in below.
        let env = unsafe { crate::alloc::cirka_alloc(16) } as Obj;
        // SAFETY: as above.
        unsafe {
            (*env).rc = 1;
            (*env).tag = 0;
            (*env).nptr = 0;
            (*env).kind = crate::object::CON;
        }
        // SAFETY: this thread's statistics, read between the calls.
        let frees = unsafe { crate::alloc::stats().frees };
        // SAFETY: a live object; the stub is never called.
        let f = unsafe { cirka_callback_new(add_env as *mut u8, env) };
        // SAFETY: the stub's reference keeps the object alive.
        assert_eq!(unsafe { (*env).rc }, -1, "shared");
        cirka_callback_free(f);
        // SAFETY: as above.
        assert_eq!(unsafe { crate::alloc::stats().frees }, frees + 1);
    }
}
