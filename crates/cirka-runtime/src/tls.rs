//! Per-thread runtime state (05-runtime.md §9.2).
//!
//! Every thread has one zero-initialized block in `.tbss`, found from the
//! thread pointer with the local-exec model (`fs:0` plus the block's
//! link-time offset). Stable Rust without `std` has no `#[thread_local]`,
//! so the block is defined with `global_asm!` and addressed with `asm!`.
//! The runtime is linked statically into the executable, which local-exec
//! requires.
//!
//! The block holds the thread's allocator state (free lists, statistics):
//! an allocation finds it with one load of the thread pointer, and needs
//! no check whether the process is multithreaded (05-runtime.md §9.2).

use crate::alloc::{CLASSES, Stats};
use core::sync::atomic::{AtomicU64, Ordering};

/// The number of the next thread (1 is the thread running `main`).
static NEXT_ID: AtomicU64 = AtomicU64::new(2);

/// A fresh thread number.
pub fn new_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// The allocator state of a thread.
#[repr(C)]
pub struct Heap {
    /// Free lists per size class (`alloc.rs`).
    pub free_lists: [*mut u8; CLASSES],
    /// Allocation statistics of this thread (`CIRKA_STATS`).
    pub stats: Stats,
}

/// The runtime state of one thread.
#[repr(C)]
pub struct Tls {
    /// The allocator state.
    pub heap: Heap,
    /// Thread number: 1 for the thread running `main`, set when the
    /// runtime starts a thread, or on first use (0 until then) in a thread
    /// it did not start (a foreign thread, a unit test).
    pub id: u64,
    /// Lowest and highest address of the thread's stack (0 if unknown),
    /// to report stack overflows.
    pub stack_lo: usize,
    pub stack_hi: usize,
    /// The event loop whose `cirka_loop_run` runs on this thread (the
    /// innermost; null if none): callbacks find it without capturing it.
    pub cur_loop: *mut crate::object::Header,
    /// The address of this thread's `errno` (the C library's), asked on
    /// first use (null until then): see [`errno_ptr`].
    pub errno_at: *mut core::ffi::c_int,
    /// The buffer `net.rs` reads into (null until the first read). A
    /// thread the runtime started frees it when it exits; a foreign thread
    /// keeps it, as it keeps its free lists (`alloc::thread_exit`).
    /// Next to `cur_loop` and `errno_at`: a receive uses the three.
    pub recv_buf: *mut u8,
    /// `cirka_call_1` (generated code), asked on first use (None until
    /// then): see [`call_1`]. Next to `io_spent`: the loop runs a callback
    /// with the two.
    pub call1: Option<crate::thread::Call1>,
    /// Operations of the running callback that completed at once
    /// (`evloop::spend`).
    pub io_spent: u32,
    /// The sender of the last datagram received (`net.rs`).
    pub peer_len: u32,
    pub peer: crate::sys::sockaddr_storage,
    /// Results of the last call of `os.rs` beyond its return value (the
    /// pipes of a spawn, the status of a wait, the fields of a stat).
    pub os: [i64; 6],
}

const TLS_SIZE: usize = core::mem::size_of::<Tls>();

core::arch::global_asm!(
    ".pushsection .tbss,\"awT\",@nobits",
    ".p2align 6",
    ".globl cirka_tls_block",
    ".hidden cirka_tls_block",
    ".type cirka_tls_block,@object",
    ".size cirka_tls_block,{size}",
    "cirka_tls_block:",
    ".zero {size}",
    ".popsection",
    size = const TLS_SIZE,
);

/// This thread's block. The address is constant for the lifetime of the
/// thread; the block is zero when the thread starts.
#[inline(always)]
pub fn tls() -> *mut Tls {
    let p: *mut Tls;
    // SAFETY: reads the thread pointer (`fs:0` holds its own address on
    // x86-64 Linux) and adds the block's offset in the executable's TLS
    // segment; no memory is written. `nomem`: the word at `fs:0` is not
    // Rust-visible memory and never changes while the thread runs, so the
    // compiler may reuse the result within a function (as for a native
    // `#[thread_local]` access).
    unsafe {
        core::arch::asm!(
            "movq %fs:0, {p}",
            "leaq cirka_tls_block@tpoff({p}), {p}",
            p = out(reg) p,
            options(att_syntax, nostack, preserves_flags, pure, nomem),
        );
    }
    p
}

/// The allocator state of the calling thread.
#[inline(always)]
pub fn heap() -> *mut Heap {
    // SAFETY: a field of this thread's block.
    unsafe { &raw mut (*tls()).heap }
}

/// This thread's number (see [`Tls::id`]).
#[inline(always)]
pub fn thread_id() -> u64 {
    // SAFETY: the block of the current thread, used by this thread only.
    let id = unsafe { (*tls()).id };
    if id != 0 { id } else { assign_id() }
}

/// Numbers a thread the runtime did not start, on its first use.
#[cold]
#[inline(never)]
fn assign_id() -> u64 {
    let id = new_id();
    // SAFETY: as for thread_id.
    unsafe { (*tls()).id = id };
    id
}

/// The C ABI entry through which the runtime applies a closure
/// (`cirka_call_1`, generated). The runtime reaches a function of the
/// generated code through the GOT, which lies on a page of its own, so the
/// address is asked once per thread and kept in the block: the loop calls
/// a callback with a load from the block (FEAT-1026).
#[inline(always)]
pub fn call_1() -> crate::thread::Call1 {
    let t = tls();
    // SAFETY: the block of the current thread, used by this thread only.
    match unsafe { (*t).call1 } {
        Some(f) => f,
        None => call_1_init(t),
    }
}

#[cold]
#[inline(never)]
fn call_1_init(t: *mut Tls) -> crate::thread::Call1 {
    let f = crate::thread::call_1_entry();
    // SAFETY: as for call_1.
    unsafe { (*t).call1 = Some(f) };
    f
}

/// The address of this thread's `errno`. The C library's `errno` is a
/// thread-local variable whose address does not change while the thread
/// runs: it is asked once (`__errno_location`) and kept in the block, so
/// that reading or setting `errno` is a load from the block instead of a
/// call into the C library, whose code lies on a page of its own
/// (FEAT-1021).
#[inline(always)]
pub fn errno_ptr() -> *mut core::ffi::c_int {
    let t = tls();
    // SAFETY: the block of the current thread, used by this thread only.
    let p = unsafe { (*t).errno_at };
    if p.is_null() { errno_init(t) } else { p }
}

/// Asks for the address of `errno` on the first use in a thread.
#[cold]
#[inline(never)]
fn errno_init(t: *mut Tls) -> *mut core::ffi::c_int {
    // SAFETY: the C library's errno of this thread; the block of the
    // current thread.
    unsafe {
        let p = crate::sys::__errno_location();
        (*t).errno_at = p;
        p
    }
}

/// This thread's `errno`.
#[inline(always)]
pub fn errno() -> i32 {
    // SAFETY: the address of this thread's errno.
    unsafe { *errno_ptr() }
}

/// Sets this thread's `errno`.
#[inline(always)]
pub fn set_errno(e: i32) {
    // SAFETY: the address of this thread's errno.
    unsafe { *errno_ptr() = e }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_thread_has_its_own_zeroed_block() {
        let a = tls() as usize;
        // SAFETY: the current thread's block.
        unsafe { (*tls()).stack_lo = 7 };
        let (b, lo) = std::thread::spawn(|| {
            // SAFETY: the new thread's block.
            (tls() as usize, unsafe { (*tls()).stack_lo })
        })
        .join()
        .unwrap();
        assert_ne!(a, b);
        assert_eq!(lo, 0, "a new thread starts with a zeroed block");
        // SAFETY: the current thread's block.
        assert_eq!(unsafe { (*tls()).stack_lo }, 7);
        assert_eq!(a % 64, 0, "the block is aligned");
    }

    /// `errno` through the address kept in the block is the C library's
    /// `errno` of the thread, in each thread (FEAT-1021).
    #[test]
    fn errno_is_the_c_librarys_of_each_thread() {
        // SAFETY: this thread's errno location.
        let c = || unsafe { *crate::sys::__errno_location() };
        set_errno(17);
        assert_eq!(c(), 17);
        // SAFETY: as above.
        unsafe { *crate::sys::__errno_location() = 23 };
        assert_eq!(errno(), 23);
        let (other, mine) = std::thread::spawn(move || {
            // SAFETY: the new thread's block, still zero.
            assert!(unsafe { (*tls()).errno_at }.is_null());
            set_errno(5);
            // SAFETY: the new thread's errno location.
            (errno(), unsafe { *crate::sys::__errno_location() })
        })
        .join()
        .unwrap();
        assert_eq!((other, mine), (5, 5));
        assert_eq!(errno(), 23, "the other thread's errno is its own");
        assert_eq!(errno_ptr(), unsafe { crate::sys::__errno_location() });
    }
}
