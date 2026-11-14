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
}
