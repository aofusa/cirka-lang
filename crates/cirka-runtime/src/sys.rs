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
pub const SS_DISABLE: c_int = 2;

#[repr(C)]
pub struct pthread_attr_t {
    _data: [u64; 7],
}

/// glibc's `pthread_mutex_t` on x86-64 (40 bytes; all zero is
/// `PTHREAD_MUTEX_INITIALIZER`).
#[repr(C)]
pub struct pthread_mutex_t {
    _data: [u64; 5],
}

impl pthread_mutex_t {
    pub const INIT: pthread_mutex_t = pthread_mutex_t { _data: [0; 5] };
}

/// glibc's `pthread_cond_t` on x86-64 (48 bytes; all zero is
/// `PTHREAD_COND_INITIALIZER`).
#[repr(C)]
pub struct pthread_cond_t {
    _data: [u64; 6],
}

impl pthread_cond_t {
    pub const INIT: pthread_cond_t = pthread_cond_t { _data: [0; 6] };
}

pub const PTHREAD_CREATE_DETACHED: c_int = 1;
pub const SC_NPROCESSORS_ONLN: c_int = 84;

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
pub struct timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

// ---- epoll, eventfd, sockets, resource limits (05-runtime.md §10) ----

pub const EPOLL_CLOEXEC: c_int = 0o2000000;
pub const EPOLL_CTL_ADD: c_int = 1;
pub const EPOLL_CTL_DEL: c_int = 2;
pub const EPOLL_CTL_MOD: c_int = 3;
pub const EPOLLIN: u32 = 0x001;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
pub const EPOLLRDHUP: u32 = 0x2000;
pub const EPOLLONESHOT: u32 = 1 << 30;

/// `struct epoll_event`, packed on x86-64 (12 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct epoll_event {
    pub events: u32,
    pub data: u64,
}

pub const EFD_CLOEXEC: c_int = 0o2000000;
pub const EFD_NONBLOCK: c_int = 0o4000;

pub const AF_INET: c_int = 2;
pub const AF_INET6: c_int = 10;
pub const SOCK_STREAM: c_int = 1;
pub const SOCK_NONBLOCK: c_int = 0o4000;
pub const SOCK_CLOEXEC: c_int = 0o2000000;
pub const SOL_SOCKET: c_int = 1;
pub const SO_REUSEADDR: c_int = 2;
pub const SO_ERROR: c_int = 4;
pub const IPPROTO_TCP: c_int = 6;
pub const TCP_NODELAY: c_int = 1;
pub const F_GETFL: c_int = 3;
pub const F_SETFL: c_int = 4;
pub const O_NONBLOCK: c_int = 0o4000;
pub const RLIMIT_NOFILE: c_int = 7;

/// Large enough for any socket address (`struct sockaddr_storage`).
#[repr(C, align(8))]
pub struct sockaddr_storage {
    pub family: u16,
    pub data: [u8; 126],
}

#[repr(C)]
pub struct sockaddr_in {
    pub sin_family: u16,
    /// Network byte order.
    pub sin_port: u16,
    pub sin_addr: [u8; 4],
    pub sin_zero: [u8; 8],
}

#[repr(C)]
pub struct sockaddr_in6 {
    pub sin6_family: u16,
    /// Network byte order.
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: [u8; 16],
    pub sin6_scope_id: u32,
}

#[repr(C)]
pub struct rlimit {
    pub rlim_cur: u64,
    pub rlim_max: u64,
}

#[repr(C)]
pub struct addrinfo {
    pub ai_flags: c_int,
    pub ai_family: c_int,
    pub ai_socktype: c_int,
    pub ai_protocol: c_int,
    pub ai_addrlen: u32,
    pub ai_addr: *mut sockaddr_storage,
    pub ai_canonname: *mut c_char,
    pub ai_next: *mut addrinfo,
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
    pub fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    pub fn close(fd: c_int) -> c_int;
    pub fn unlink(path: *const c_char) -> c_int;
    pub fn __errno_location() -> *mut c_int;
    pub fn getenv(name: *const c_char) -> *const c_char;
    pub fn clock_gettime(clock: c_int, ts: *mut timespec) -> c_int;
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
    pub fn pthread_attr_setdetachstate(a: *mut pthread_attr_t, state: c_int) -> c_int;
    pub fn pthread_attr_destroy(a: *mut pthread_attr_t) -> c_int;
    pub fn pthread_mutex_init(m: *mut pthread_mutex_t, attr: *const c_void) -> c_int;
    pub fn pthread_mutex_destroy(m: *mut pthread_mutex_t) -> c_int;
    pub fn pthread_mutex_lock(m: *mut pthread_mutex_t) -> c_int;
    pub fn pthread_mutex_unlock(m: *mut pthread_mutex_t) -> c_int;
    pub fn pthread_mutex_trylock(m: *mut pthread_mutex_t) -> c_int;
    pub fn pthread_cond_init(c: *mut pthread_cond_t, attr: *const c_void) -> c_int;
    pub fn pthread_cond_destroy(c: *mut pthread_cond_t) -> c_int;
    pub fn pthread_cond_wait(c: *mut pthread_cond_t, m: *mut pthread_mutex_t) -> c_int;
    pub fn pthread_cond_broadcast(c: *mut pthread_cond_t) -> c_int;
    pub fn sched_yield() -> c_int;
    pub fn sysconf(name: c_int) -> c_long;
    pub fn realloc(p: *mut c_void, size: size_t) -> *mut c_void;
    pub fn sigaltstack(ss: *const stack_t, old: *mut stack_t) -> c_int;
    pub fn sigaction(sig: c_int, act: *const sigaction_t, old: *mut sigaction_t) -> c_int;
    pub fn pthread_cond_signal(c: *mut pthread_cond_t) -> c_int;
    pub fn epoll_create1(flags: c_int) -> c_int;
    pub fn epoll_ctl(epfd: c_int, op: c_int, fd: c_int, ev: *mut epoll_event) -> c_int;
    pub fn epoll_wait(epfd: c_int, evs: *mut epoll_event, max: c_int, timeout: c_int) -> c_int;
    pub fn eventfd(init: u32, flags: c_int) -> c_int;
    pub fn socket(domain: c_int, ty: c_int, proto: c_int) -> c_int;
    pub fn setsockopt(fd: c_int, level: c_int, name: c_int, val: *const c_void, len: u32) -> c_int;
    pub fn getsockopt(
        fd: c_int,
        level: c_int,
        name: c_int,
        val: *mut c_void,
        len: *mut u32,
    ) -> c_int;
    pub fn bind(fd: c_int, addr: *const sockaddr_storage, len: u32) -> c_int;
    pub fn listen(fd: c_int, backlog: c_int) -> c_int;
    pub fn accept4(fd: c_int, addr: *mut sockaddr_storage, len: *mut u32, flags: c_int) -> c_int;
    pub fn connect(fd: c_int, addr: *const sockaddr_storage, len: u32) -> c_int;
    pub fn getsockname(fd: c_int, addr: *mut sockaddr_storage, len: *mut u32) -> c_int;
    pub fn shutdown(fd: c_int, how: c_int) -> c_int;
    pub fn send(fd: c_int, buf: *const c_void, n: size_t, flags: c_int) -> ssize_t;
    pub fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    pub fn getrlimit(res: c_int, rl: *mut rlimit) -> c_int;
    pub fn setrlimit(res: c_int, rl: *const rlimit) -> c_int;
    pub fn inet_pton(af: c_int, src: *const c_char, dst: *mut c_void) -> c_int;
    pub fn inet_ntop(af: c_int, src: *const c_void, dst: *mut c_char, size: u32) -> *const c_char;
    pub fn getaddrinfo(
        node: *const c_char,
        service: *const c_char,
        hints: *const addrinfo,
        res: *mut *mut addrinfo,
    ) -> c_int;
    pub fn freeaddrinfo(res: *mut addrinfo);
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
