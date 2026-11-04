//! Minimal libc bindings (the runtime depends only on libc).

#![allow(non_camel_case_types)]

use core::ffi::{c_char, c_int, c_long, c_void};

pub type size_t = usize;
pub type ssize_t = isize;
pub type pthread_t = u64;

pub const PROT_READ: c_int = 1;
pub const PROT_WRITE: c_int = 2;
pub const MAP_PRIVATE: c_int = 0x02;
pub const MAP_ANONYMOUS: c_int = 0x20;
pub const MAP_NORESERVE: c_int = 0x4000;
pub const MAP_FAILED: *mut c_void = !0usize as *mut c_void;

pub const SIGSEGV: c_int = 11;
pub const SIGBUS: c_int = 7;
pub const SA_SIGINFO: c_int = 4;
pub const SA_ONSTACK: c_int = 0x0800_0000;

#[repr(C)]
pub struct pthread_attr_t {
    _data: [u64; 7],
}

#[repr(C)]
pub struct stack_t {
    pub ss_sp: *mut c_void,
    pub ss_flags: c_int,
    pub ss_size: size_t,
}

#[repr(C)]
pub struct sigaction_t {
    pub sa_sigaction: usize,
    pub sa_mask: [u64; 16],
    pub sa_flags: c_int,
    pub sa_restorer: usize,
}

#[repr(C)]
pub struct siginfo_t {
    pub si_signo: c_int,
    pub si_errno: c_int,
    pub si_code: c_int,
    _pad: c_int,
    pub si_addr: *mut c_void,
    _rest: [u64; 12],
}

unsafe extern "C" {
    pub fn malloc(size: size_t) -> *mut c_void;
    pub fn calloc(n: size_t, size: size_t) -> *mut c_void;
    pub fn free(p: *mut c_void);
    pub fn mmap(
        addr: *mut c_void,
        len: size_t,
        prot: c_int,
        flags: c_int,
        fd: c_int,
        off: c_long,
    ) -> *mut c_void;
    pub fn write(fd: c_int, buf: *const c_void, n: size_t) -> ssize_t;
    pub fn read(fd: c_int, buf: *mut c_void, n: size_t) -> ssize_t;
    pub fn getenv(name: *const c_char) -> *const c_char;
    pub fn _exit(code: c_int) -> !;
    pub fn abort() -> !;
    pub fn memset(p: *mut c_void, c: c_int, n: size_t) -> *mut c_void;
    pub fn pthread_attr_init(a: *mut pthread_attr_t) -> c_int;
    pub fn pthread_attr_setstacksize(a: *mut pthread_attr_t, size: size_t) -> c_int;
    pub fn pthread_attr_setguardsize(a: *mut pthread_attr_t, size: size_t) -> c_int;
    pub fn pthread_create(
        t: *mut pthread_t,
        a: *const pthread_attr_t,
        f: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> c_int;
    pub fn pthread_join(t: pthread_t, ret: *mut *mut c_void) -> c_int;
    pub fn sigaltstack(ss: *const stack_t, old: *mut stack_t) -> c_int;
    pub fn sigaction(sig: c_int, act: *const sigaction_t, old: *mut sigaction_t) -> c_int;
}

/// Reads an environment variable as a byte slice (without the NUL).
pub fn env(name: &core::ffi::CStr) -> Option<&'static [u8]> {
    // SAFETY: getenv returns NULL or a NUL-terminated string that lives as
    // long as the environment is not modified (the runtime never modifies it).
    unsafe {
        let p = getenv(name.as_ptr());
        if p.is_null() {
            return None;
        }
        Some(core::ffi::CStr::from_ptr(p).to_bytes())
    }
}

/// Parses sizes such as `512M`, `64k`, `1G` or plain bytes.
pub fn parse_size(s: &[u8]) -> Option<usize> {
    let (digits, mult) = match s.last()? {
        b'k' | b'K' => (&s[..s.len() - 1], 1usize << 10),
        b'm' | b'M' => (&s[..s.len() - 1], 1 << 20),
        b'g' | b'G' => (&s[..s.len() - 1], 1 << 30),
        _ => (s, 1),
    };
    if digits.is_empty() {
        return None;
    }
    let mut n: usize = 0;
    for &d in digits {
        if !d.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add((d - b'0') as usize)?;
    }
    n.checked_mul(mult)
}
