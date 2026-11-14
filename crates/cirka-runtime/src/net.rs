//! Sockets and descriptor helpers for event loops (05-runtime.md §10.2,
//! FEAT-0046).
//!
//! Sockets are non-blocking and close-on-exec. Failures return `-errno`
//! (the convention of `stdio.rs`). Descriptors are plain integers, or
//! descriptor objects (`cirka_fd_handle`) that close them when freed: the
//! libraries hold those in linear values (spec-v0.0.5.md §2), and a
//! continuation dropped with its loop still closes its socket.

use crate::evloop::{budget_left, foreign, spend};
use crate::object::{FOREIGN, Obj, word};
use crate::{io, sys};
use core::ffi::{c_char, c_int, c_void};
use core::mem::MaybeUninit;

const EINTR: i32 = 4;
const ENOENT: i32 = 2;
const EBADF: i32 = 9;
const EAGAIN: i32 = 11;
const EINVAL: i64 = 22;
const EINPROGRESS: i32 = 115;
const MSG_NOSIGNAL: c_int = 0x4000;

/// The most bytes one `cirka_sock_recv` / `cirka_sock_recvfrom` reads
/// (a datagram is at most 65507 bytes).
const RECV_MAX: usize = 65536;

fn errno() -> i32 {
    // SAFETY: the thread's errno location.
    unsafe { *sys::__errno_location() }
}

fn set_errno(e: i32) {
    // SAFETY: the thread's errno location.
    unsafe { *sys::__errno_location() = e }
}

fn neg_errno() -> i64 {
    -i64::from(errno())
}

/// `s` as a NUL-terminated string in a stack buffer (addresses are short).
/// `None` if it is too long or contains a NUL.
///
/// # Safety
/// `s` must be a live BYTES object.
unsafe fn small_c_string(s: Obj, buf: &mut [u8; 256]) -> Option<*const c_char> {
    // SAFETY: guaranteed by the caller.
    let b = unsafe { io::bytes_of(s) };
    if b.len() >= buf.len() || b.contains(&0) {
        return None;
    }
    buf[..b.len()].copy_from_slice(b);
    buf[b.len()] = 0;
    Some(buf.as_ptr() as *const c_char)
}

/// A numeric IPv4 or IPv6 address and `port` as a socket address (an
/// empty host is the IPv4 wildcard address).
///
/// # Safety
/// `host` must be a live BYTES object.
unsafe fn sockaddr(host: Obj, port: i64) -> Result<(sys::sockaddr_storage, u32), i64> {
    if !(0..=65535).contains(&port) {
        return Err(-EINVAL);
    }
    let mut buf = [0u8; 256];
    // SAFETY: `host` is live; inet_pton writes at most 16 bytes into the
    // address fields of the zeroed storage.
    unsafe {
        let mut sa: sys::sockaddr_storage = core::mem::zeroed();
        if io::bytes_of(host).is_empty() {
            let a = &mut sa as *mut _ as *mut sys::sockaddr_in;
            (*a).sin_family = sys::AF_INET as u16;
            (*a).sin_port = (port as u16).to_be();
            return Ok((sa, size_of::<sys::sockaddr_in>() as u32));
        }
        let Some(h) = small_c_string(host, &mut buf) else {
            return Err(-EINVAL);
        };
        let a4 = &mut sa as *mut _ as *mut sys::sockaddr_in;
        if sys::inet_pton(sys::AF_INET, h, (*a4).sin_addr.as_mut_ptr() as *mut c_void) == 1 {
            (*a4).sin_family = sys::AF_INET as u16;
            (*a4).sin_port = (port as u16).to_be();
            return Ok((sa, size_of::<sys::sockaddr_in>() as u32));
        }
        let a6 = &mut sa as *mut _ as *mut sys::sockaddr_in6;
        if sys::inet_pton(
            sys::AF_INET6,
            h,
            (*a6).sin6_addr.as_mut_ptr() as *mut c_void,
        ) == 1
        {
            (*a6).sin6_family = sys::AF_INET6 as u16;
            (*a6).sin6_port = (port as u16).to_be();
            return Ok((sa, size_of::<sys::sockaddr_in6>() as u32));
        }
        Err(-EINVAL)
    }
}

/// A new non-blocking, close-on-exec TCP socket of the address's family.
fn tcp_socket(sa: &sys::sockaddr_storage) -> Result<c_int, i64> {
    new_socket(sa, sys::SOCK_STREAM)
}

/// A new non-blocking, close-on-exec socket of the address's family.
fn new_socket(sa: &sys::sockaddr_storage, ty: c_int) -> Result<c_int, i64> {
    // SAFETY: plain system call.
    let fd = unsafe {
        sys::socket(
            c_int::from(sa.family),
            ty | sys::SOCK_NONBLOCK | sys::SOCK_CLOEXEC,
            0,
        )
    };
    if fd < 0 { Err(neg_errno()) } else { Ok(fd) }
}

/// Closes `fd` after a failure, keeping the failure's errno.
fn close_failed(fd: c_int) -> i64 {
    let e = neg_errno();
    // SAFETY: the caller's own descriptor.
    unsafe { sys::close(fd) };
    e
}

/// A TCP socket listening on `host` (numeric; empty = any IPv4 address)
/// and `port` (0 = any free port), with `SO_REUSEADDR`. `backlog` <= 0
/// uses the system's maximum. The descriptor, or `-errno`.
///
/// # Safety
/// `host` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_net_listen(host: Obj, port: i64, backlog: i64) -> i64 {
    // SAFETY: `host` is live; the socket calls get valid addresses.
    unsafe {
        let (sa, len) = match sockaddr(host, port) {
            Ok(a) => a,
            Err(e) => return e,
        };
        let fd = match tcp_socket(&sa) {
            Ok(fd) => fd,
            Err(e) => return e,
        };
        let one: c_int = 1;
        if sys::setsockopt(
            fd,
            sys::SOL_SOCKET,
            sys::SO_REUSEADDR,
            &one as *const c_int as *const c_void,
            4,
        ) < 0
            || sys::bind(fd, &sa, len) < 0
        {
            return close_failed(fd);
        }
        let backlog = if backlog <= 0 {
            4096
        } else {
            backlog.min(65535) as c_int
        };
        if sys::listen(fd, backlog) < 0 {
            return close_failed(fd);
        }
        i64::from(fd)
    }
}

/// Accepts a connection on the listening socket `fd`: the new socket
/// (non-blocking), or `-errno` (`-EAGAIN` when none is pending).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_net_accept(fd: i64) -> i64 {
    loop {
        // SAFETY: plain system call; no address is returned.
        let c = unsafe {
            sys::accept4(
                fd as c_int,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                sys::SOCK_NONBLOCK | sys::SOCK_CLOEXEC,
            )
        };
        if c >= 0 {
            return i64::from(c);
        }
        if errno() != EINTR {
            return neg_errno();
        }
    }
}

/// Starts connecting a TCP socket to `host` (numeric) and `port`. Returns
/// the socket without waiting: when it becomes writable,
/// `cirka_net_connect_result` tells whether the connection succeeded.
/// `-errno` if it failed at once.
///
/// # Safety
/// `host` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_net_connect(host: Obj, port: i64) -> i64 {
    // SAFETY: `host` is live; valid address.
    unsafe {
        let (sa, len) = match sockaddr(host, port) {
            Ok(a) => a,
            Err(e) => return e,
        };
        let fd = match tcp_socket(&sa) {
            Ok(fd) => fd,
            Err(e) => return e,
        };
        loop {
            if sys::connect(fd, &sa, len) == 0 {
                break;
            }
            match errno() {
                EINTR => continue,
                EINPROGRESS => break,
                _ => return close_failed(fd),
            }
        }
        i64::from(fd)
    }
}

/// The outcome of a connection started by `cirka_net_connect`, once the
/// socket is writable: 0, or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_net_connect_result(fd: i64) -> i64 {
    let mut err: c_int = 0;
    let mut len: u32 = 4;
    // SAFETY: valid out pointers.
    let r = unsafe {
        sys::getsockopt(
            fd as c_int,
            sys::SOL_SOCKET,
            sys::SO_ERROR,
            &mut err as *mut c_int as *mut c_void,
            &mut len,
        )
    };
    if r < 0 { neg_errno() } else { -i64::from(err) }
}

/// The local port of a socket, or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_net_local_port(fd: i64) -> i64 {
    // SAFETY: a zeroed storage large enough for any address.
    unsafe {
        let mut sa: sys::sockaddr_storage = core::mem::zeroed();
        let mut len = size_of::<sys::sockaddr_storage>() as u32;
        if sys::getsockname(fd as c_int, &mut sa, &mut len) < 0 {
            return neg_errno();
        }
        // The port is at the same offset in IPv4 and IPv6 addresses.
        let a = &sa as *const _ as *const sys::sockaddr_in;
        i64::from(u16::from_be((*a).sin_port))
    }
}

/// Turns Nagle's algorithm off (`on` = 1) or on: 0, or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_net_set_nodelay(fd: i64, on: i64) -> i64 {
    let v: c_int = c_int::from(on != 0);
    // SAFETY: valid option value.
    let r = unsafe {
        sys::setsockopt(
            fd as c_int,
            sys::IPPROTO_TCP,
            sys::TCP_NODELAY,
            &v as *const c_int as *const c_void,
            4,
        )
    };
    if r < 0 { neg_errno() } else { 0 }
}

/// Shuts down reading (0), writing (1) or both (2): 0, or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_net_shutdown(fd: i64, how: i64) -> i64 {
    if !(0..=2).contains(&how) {
        return -EINVAL;
    }
    // SAFETY: plain system call.
    let r = unsafe { sys::shutdown(fd as c_int, how as c_int) };
    if r < 0 { neg_errno() } else { 0 }
}

/// The first address of `host` as text (blocks: call it from blocking
/// work). Empty, with `errno` = ENOENT, if the name is unknown.
///
/// # Safety
/// `host` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_net_resolve(host: Obj) -> Obj {
    let mut buf = [0u8; 256];
    let mut text = [0u8; 64];
    // SAFETY: valid strings and out pointers; the result list is freed.
    unsafe {
        let mut len = 0usize;
        if let Some(h) = small_c_string(host, &mut buf) {
            let mut hints: sys::addrinfo = core::mem::zeroed();
            hints.ai_socktype = sys::SOCK_STREAM;
            let mut res: *mut sys::addrinfo = core::ptr::null_mut();
            if sys::getaddrinfo(h, core::ptr::null(), &hints, &mut res) == 0 && !res.is_null() {
                let sa = (*res).ai_addr;
                let src = if c_int::from((*sa).family) == sys::AF_INET {
                    (*(sa as *const sys::sockaddr_in)).sin_addr.as_ptr() as *const c_void
                } else {
                    (*(sa as *const sys::sockaddr_in6)).sin6_addr.as_ptr() as *const c_void
                };
                if !sys::inet_ntop(
                    c_int::from((*sa).family),
                    src,
                    text.as_mut_ptr() as *mut c_char,
                    text.len() as u32,
                )
                .is_null()
                {
                    len = text.iter().position(|&c| c == 0).unwrap_or(0);
                }
                sys::freeaddrinfo(res);
            }
        }
        set_errno(if len == 0 { ENOENT } else { 0 });
        crate::cirka_bytes_from(text.as_ptr(), len)
    }
}

/// Writes as much of `data` from byte `off` on as `fd` takes now (one
/// `write`): the number of bytes written, or `-errno` (`-EAGAIN` when a
/// non-blocking descriptor is full).
///
/// # Safety
/// `data` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fd_write_from(fd: i64, data: Obj, off: i64) -> i64 {
    if fd == 1 || fd == 2 {
        io::flush();
    }
    // SAFETY: `data` is live; the range is checked.
    unsafe {
        let Some(d) = from(data, off) else {
            return -EINVAL;
        };
        loop {
            let n = sys::write(fd as c_int, d.as_ptr() as *const c_void, d.len());
            if n >= 0 {
                return n as i64;
            }
            if errno() != EINTR {
                return neg_errno();
            }
        }
    }
}

/// Like `cirka_fd_write_from` for a socket, with `MSG_NOSIGNAL`: a peer
/// that closed the connection is `-EPIPE` (or `-ECONNRESET`), not a
/// `SIGPIPE` that ends the process.
///
/// # Safety
/// `data` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_net_send(fd: i64, data: Obj, off: i64) -> i64 {
    // SAFETY: `data` is live; the range is checked.
    unsafe {
        let Some(d) = from(data, off) else {
            return -EINVAL;
        };
        loop {
            let n = sys::send(
                fd as c_int,
                d.as_ptr() as *const c_void,
                d.len(),
                MSG_NOSIGNAL,
            );
            if n >= 0 {
                return n as i64;
            }
            if errno() != EINTR {
                return neg_errno();
            }
        }
    }
}

/// The bytes of `data` from `off` on, if `off` is within them.
///
/// # Safety
/// `data` must be a live BYTES object.
unsafe fn from<'a>(data: Obj, off: i64) -> Option<&'a [u8]> {
    // SAFETY: guaranteed by the caller.
    let d = unsafe { io::bytes_of(data) };
    if off < 0 || off as usize > d.len() {
        return None;
    }
    Some(&d[off as usize..])
}

/// Makes `fd` non-blocking (`on` = 1) or blocking: 0, or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_fd_set_nonblocking(fd: i64, on: i64) -> i64 {
    // SAFETY: plain system calls.
    unsafe {
        let fl = sys::fcntl(fd as c_int, sys::F_GETFL);
        if fl < 0 {
            return neg_errno();
        }
        let nf = if on != 0 {
            fl | sys::O_NONBLOCK
        } else {
            fl & !sys::O_NONBLOCK
        };
        if sys::fcntl(fd as c_int, sys::F_SETFL, nf) < 0 {
            return neg_errno();
        }
    }
    0
}

/// Raises the soft limit of open descriptors to the hard limit (at most
/// 2^20): the new limit, or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_fd_limit_raise() -> i64 {
    // SAFETY: valid in/out pointers.
    unsafe {
        let mut rl = sys::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if sys::getrlimit(sys::RLIMIT_NOFILE, &mut rl) < 0 {
            return neg_errno();
        }
        let want = rl.rlim_max.min(1 << 20);
        if rl.rlim_cur < want {
            rl.rlim_cur = want;
            if sys::setrlimit(sys::RLIMIT_NOFILE, &rl) < 0 {
                return neg_errno();
            }
        }
        rl.rlim_cur as i64
    }
}

// ---- Descriptor objects and the operations of the libraries ----

/// Word 0 of a descriptor object whose descriptor was closed.
const CLOSED: usize = usize::MAX;

/// The finalizer of a descriptor object: closes the descriptor, unless
/// `cirka_handle_close` did.
extern "C" fn handle_finalize(fd: usize) {
    if fd != CLOSED {
        // SAFETY: the object owned the descriptor.
        unsafe { sys::close(fd as c_int) };
    }
}

/// A descriptor object (kind `FOREIGN`) owning `fd` >= 0: the descriptor
/// is closed when the object is freed.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_fd_handle(fd: i64) -> Obj {
    if fd < 0 {
        io::fatal(b"cirka_fd_handle: not a descriptor", 2);
    }
    foreign(fd as usize as *mut c_void, handle_finalize)
}

/// The word holding the descriptor of the descriptor object `h`.
///
/// # Safety
/// `h` must be a live object.
unsafe fn handle_word(h: Obj) -> *mut usize {
    // SAFETY: a FOREIGN object has its pointer in word 0 and its finalizer
    // in word 1.
    unsafe {
        if (*h).kind != FOREIGN || *word(h, 1) != handle_finalize as extern "C" fn(usize) as usize {
            io::fatal(b"not a descriptor object", 2);
        }
        word(h, 0)
    }
}

/// The descriptor of the descriptor object `h`, or -1 once it is closed.
///
/// # Safety
/// `h` must be a live object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_handle_fd(h: Obj) -> i64 {
    // SAFETY: guaranteed by the caller.
    let fd = unsafe { *handle_word(h) };
    if fd == CLOSED { -1 } else { fd as i64 }
}

/// Closes the descriptor of `h` now (freeing `h` will not): 0, or
/// `-errno` (`-EBADF` if it was closed already).
///
/// # Safety
/// `h` must be a live object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_handle_close(h: Obj) -> i64 {
    // SAFETY: guaranteed by the caller; the descriptor is closed once.
    unsafe {
        let w = handle_word(h);
        let fd = *w;
        if fd == CLOSED {
            return -i64::from(EBADF);
        }
        *w = CLOSED;
        if sys::close(fd as c_int) < 0 && errno() != EINTR {
            return neg_errno();
        }
        0
    }
}

/// Reads with `f` into a buffer of at most `max` bytes (1 to 64 KiB) and
/// returns the bytes read, with `errno` 0; empty with `errno` 0 when `f`
/// returns 0, and empty with the error (`EAGAIN` also when the budget of
/// the running callback is spent).
fn read_with(fd: i64, max: i64, mut f: impl FnMut(c_int, *mut c_void, usize) -> isize) -> Obj {
    let mut buf = [const { MaybeUninit::<u8>::uninit() }; RECV_MAX];
    let n = (max.max(1) as usize).min(RECV_MAX);
    let mut err = 0;
    let mut len = 0;
    if fd < 0 {
        err = EBADF;
    } else if !budget_left() {
        err = EAGAIN;
    } else {
        loop {
            let r = f(fd as c_int, buf.as_mut_ptr() as *mut c_void, n);
            if r >= 0 {
                len = r as usize;
                spend();
                break;
            }
            if errno() != EINTR {
                err = errno();
                break;
            }
        }
    }
    set_errno(err);
    // SAFETY: the first `len` bytes were written by the read.
    unsafe { crate::cirka_bytes_from(buf.as_ptr() as *const u8, len) }
}

/// Reads at most `max` bytes from the socket of `h`: the bytes (with
/// `errno` 0); empty with `errno` 0 at the end of the stream; empty with
/// `EAGAIN` when nothing is there yet or the budget of the running
/// callback is spent; empty with the error otherwise.
///
/// # Safety
/// `h` must be a live descriptor object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_sock_recv(h: Obj, max: i64) -> Obj {
    // SAFETY: guaranteed by the caller.
    let fd = unsafe { cirka_handle_fd(h) };
    // SAFETY: `buf` has room for `n` bytes.
    read_with(fd, max, |fd, buf, n| unsafe { sys::read(fd, buf, n) })
}

/// `cirka_net_send` on the socket of `h`: `-EAGAIN` also when the budget
/// of the running callback is spent.
///
/// # Safety
/// `h` must be a live descriptor object, `data` a live BYTES object
/// (both borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_sock_send(h: Obj, data: Obj, off: i64) -> i64 {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let fd = cirka_handle_fd(h);
        if fd < 0 {
            return -i64::from(EBADF);
        }
        if !budget_left() {
            return -i64::from(EAGAIN);
        }
        let r = cirka_net_send(fd, data, off);
        if r >= 0 {
            spend();
        }
        r
    }
}

/// `cirka_net_accept` on the listening socket of `h`: `-EAGAIN` also when
/// the budget of the running callback is spent.
///
/// # Safety
/// `h` must be a live descriptor object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_sock_accept(h: Obj) -> i64 {
    // SAFETY: guaranteed by the caller.
    let fd = unsafe { cirka_handle_fd(h) };
    if fd < 0 {
        return -i64::from(EBADF);
    }
    if !budget_left() {
        return -i64::from(EAGAIN);
    }
    let r = cirka_net_accept(fd);
    if r >= 0 {
        spend();
    }
    r
}

/// A UDP socket bound to `host` (numeric; empty = any IPv4 address) and
/// `port` (0 = any free port): the descriptor, or `-errno`.
///
/// # Safety
/// `host` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_net_udp_bind(host: Obj, port: i64) -> i64 {
    // SAFETY: `host` is live; the socket calls get valid addresses.
    unsafe {
        let (sa, len) = match sockaddr(host, port) {
            Ok(a) => a,
            Err(e) => return e,
        };
        let fd = match new_socket(&sa, sys::SOCK_DGRAM) {
            Ok(fd) => fd,
            Err(e) => return e,
        };
        if sys::bind(fd, &sa, len) < 0 {
            return close_failed(fd);
        }
        i64::from(fd)
    }
}

/// Sends `data` as one datagram from the socket of `h` to `host`
/// (numeric) and `port`: the number of bytes sent, or `-errno` (`-EAGAIN`
/// also when the budget of the running callback is spent).
///
/// # Safety
/// `h` must be a live descriptor object; `data` and `host` live BYTES
/// objects (all borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_sock_sendto(h: Obj, data: Obj, host: Obj, port: i64) -> i64 {
    // SAFETY: guaranteed by the caller; valid address and buffer.
    unsafe {
        let fd = cirka_handle_fd(h);
        if fd < 0 {
            return -i64::from(EBADF);
        }
        let (sa, len) = match sockaddr(host, port) {
            Ok(a) => a,
            Err(e) => return e,
        };
        if !budget_left() {
            return -i64::from(EAGAIN);
        }
        let d = io::bytes_of(data);
        loop {
            let n = sys::sendto(
                fd as c_int,
                d.as_ptr() as *const c_void,
                d.len(),
                MSG_NOSIGNAL,
                &sa,
                len,
            );
            if n >= 0 {
                spend();
                return n as i64;
            }
            if errno() != EINTR {
                return neg_errno();
            }
        }
    }
}

/// Receives one datagram (at most `max` bytes are kept) on the socket of
/// `h`, as `cirka_sock_recv` (an empty datagram is empty with `errno` 0).
/// The sender is kept for `cirka_net_peer_host` / `cirka_net_peer_port`.
///
/// # Safety
/// `h` must be a live descriptor object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_sock_recvfrom(h: Obj, max: i64) -> Obj {
    // SAFETY: guaranteed by the caller; the sender goes to this thread's
    // block.
    unsafe {
        let fd = cirka_handle_fd(h);
        let t = crate::tls::tls();
        (*t).peer_len = 0;
        read_with(fd, max, |fd, buf, n| {
            let mut len = size_of::<sys::sockaddr_storage>() as u32;
            let r = sys::recvfrom(fd, buf, n, 0, &raw mut (*t).peer, &mut len);
            if r >= 0 {
                (*t).peer_len = len;
            }
            r
        })
    }
}

/// The address of the sender of the last datagram this thread received,
/// as text (empty if there is none).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_net_peer_host() -> Obj {
    let mut text = [0u8; 64];
    // SAFETY: this thread's block; inet_ntop writes at most 64 bytes.
    unsafe {
        let t = crate::tls::tls();
        let mut len = 0;
        if (*t).peer_len > 0 {
            let sa = &raw const (*t).peer;
            let fam = c_int::from((*sa).family);
            let src = if fam == sys::AF_INET {
                (*(sa as *const sys::sockaddr_in)).sin_addr.as_ptr() as *const c_void
            } else {
                (*(sa as *const sys::sockaddr_in6)).sin6_addr.as_ptr() as *const c_void
            };
            if !sys::inet_ntop(fam, src, text.as_mut_ptr() as *mut c_char, 64).is_null() {
                len = text.iter().position(|&c| c == 0).unwrap_or(0);
            }
        }
        crate::cirka_bytes_from(text.as_ptr(), len)
    }
}

/// The port of the sender of the last datagram this thread received (0
/// if there is none).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_net_peer_port() -> i64 {
    // SAFETY: this thread's block; the port is at the same offset in IPv4
    // and IPv6 addresses.
    unsafe {
        let t = crate::tls::tls();
        if (*t).peer_len == 0 {
            return 0;
        }
        let a = &raw const (*t).peer as *const sys::sockaddr_in;
        i64::from(u16::from_be((*a).sin_port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_LOCK as LOCK;
    use crate::evloop::*;
    use crate::object::cirka_drop;
    use core::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Mutex as StdMutex;

    static LOG: StdMutex<Vec<u8>> = StdMutex::new(Vec::new());
    static LISTENER: AtomicI64 = AtomicI64::new(-1);
    static CLIENT: AtomicI64 = AtomicI64::new(-1);
    static SERVER: AtomicI64 = AtomicI64::new(-1);

    fn log(b: &[u8]) {
        LOG.lock().unwrap().extend_from_slice(b);
    }

    fn closure(f: extern "C" fn(Obj) -> Obj) -> Obj {
        let code = (f as *const () as usize).to_ne_bytes();
        // SAFETY: a valid slice.
        unsafe { crate::cirka_bytes_from(code.as_ptr(), 8) }
    }

    fn text(s: &str) -> Obj {
        // SAFETY: a valid slice.
        unsafe { crate::cirka_bytes_from(s.as_ptr(), s.len()) }
    }

    /// Watches `fd` for `ev` with `f` on the running loop.
    fn watch(fd: i64, ev: i64, f: extern "C" fn(Obj) -> Obj) {
        let l = cirka_loop_current();
        let cb = closure(f);
        // SAFETY: the running loop and a live closure, both released.
        unsafe {
            assert_eq!(cirka_loop_watch(l, fd, ev, cb), 0);
            cirka_drop(cb);
            cirka_drop(l);
        }
    }

    /// Reads what `fd` has (at most 64 bytes).
    fn read(fd: i64) -> Vec<u8> {
        let d = crate::stdio::cirka_fd_read(fd, 64);
        // SAFETY: a fresh string, released after the copy.
        unsafe {
            let v = io::bytes_of(d).to_vec();
            cirka_drop(d);
            v
        }
    }

    extern "C" fn on_accept(t: Obj) -> Obj {
        let lfd = LISTENER.load(Ordering::Relaxed);
        let c = cirka_net_accept(lfd);
        assert!(c >= 0, "accept: {c}");
        assert_eq!(cirka_net_accept(lfd), -11, "no other connection (EAGAIN)");
        crate::stdio::cirka_fd_close(lfd);
        SERVER.store(c, Ordering::Relaxed);
        watch(c, EV_READ, on_server_read);
        t
    }

    extern "C" fn on_server_read(t: Obj) -> Obj {
        let c = SERVER.load(Ordering::Relaxed);
        let d = read(c);
        if d.is_empty() {
            log(b"[eof]");
            crate::stdio::cirka_fd_close(c);
        } else {
            // SAFETY: a fresh string, released after the send.
            unsafe {
                let o = crate::cirka_bytes_from(d.as_ptr(), d.len());
                assert_eq!(cirka_net_send(c, o, 0), d.len() as i64);
                cirka_drop(o);
            }
            watch(c, EV_READ, on_server_read);
        }
        t
    }

    extern "C" fn on_connected(t: Obj) -> Obj {
        let fd = CLIENT.load(Ordering::Relaxed);
        assert_eq!(cirka_net_connect_result(fd), 0);
        assert_eq!(cirka_net_set_nodelay(fd, 1), 0);
        let m = text("ping");
        // SAFETY: a fresh string, released after the send.
        unsafe {
            // From an offset: "ing".
            assert_eq!(cirka_net_send(fd, m, 1), 3);
            assert_eq!(cirka_net_send(fd, m, 5), -EINVAL);
            cirka_drop(m);
        }
        watch(fd, EV_READ, on_echo);
        t
    }

    extern "C" fn on_echo(t: Obj) -> Obj {
        let fd = CLIENT.load(Ordering::Relaxed);
        let d = read(fd);
        log(&d);
        if d.len() < 3 {
            watch(fd, EV_READ, on_echo);
        } else {
            // The server sees the end of the input and closes too.
            assert_eq!(cirka_net_shutdown(fd, 1), 0);
            crate::stdio::cirka_fd_close(fd);
        }
        t
    }

    #[test]
    fn echo_over_loopback() {
        let _g = LOCK.lock().unwrap();
        // SAFETY: a loop of this thread and live closures and strings.
        unsafe {
            let any = text("127.0.0.1");
            let lfd = cirka_net_listen(any, 0, 0);
            assert!(lfd >= 0, "listen: {lfd}");
            let port = cirka_net_local_port(lfd);
            assert!(port > 0);
            // The port is taken while the listener lives.
            assert_eq!(cirka_net_listen(any, port, 0), -98);
            LISTENER.store(lfd, Ordering::Relaxed);
            let l = cirka_loop_new();
            let acc = closure(on_accept);
            assert_eq!(cirka_loop_watch(l, lfd, EV_READ, acc), 0);
            let fd = cirka_net_connect(any, port);
            assert!(fd >= 0, "connect: {fd}");
            CLIENT.store(fd, Ordering::Relaxed);
            let con = closure(on_connected);
            assert_eq!(cirka_loop_watch(l, fd, EV_WRITE, con), 0);
            assert_eq!(cirka_loop_run(l), 0);
            assert_eq!(core::mem::take(&mut *LOG.lock().unwrap()), b"ing[eof]");
            for o in [acc, con, l, any] {
                cirka_drop(o);
            }
        }
    }

    #[test]
    fn addresses_and_errors() {
        let _g = LOCK.lock().unwrap();
        // SAFETY: live strings, released at the end.
        unsafe {
            let bad = text("not an address");
            assert_eq!(cirka_net_listen(bad, 0, 0), -EINVAL);
            let any = text("");
            assert_eq!(cirka_net_listen(any, 70000, 0), -EINVAL);
            // A port nobody listens on: refused at once or when connected.
            let v6 = text("::1");
            let l6 = cirka_net_listen(v6, 0, 16);
            let lo = text("127.0.0.1");
            let lfd = cirka_net_listen(lo, 0, 0);
            let port = cirka_net_local_port(lfd);
            crate::stdio::cirka_fd_close(lfd);
            let fd = cirka_net_connect(lo, port);
            if fd >= 0 {
                let mut tries = 0;
                while cirka_net_connect_result(fd) == 0 && tries < 1000 {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    tries += 1;
                }
                crate::stdio::cirka_fd_close(fd);
            } else {
                assert_eq!(fd, -111, "ECONNREFUSED");
            }
            assert_eq!(cirka_net_shutdown(0, 3), -EINVAL);
            assert_eq!(cirka_net_connect_result(-1), -9);
            if l6 >= 0 {
                // IPv6 loopback (absent in some containers).
                assert!(cirka_net_local_port(l6) > 0);
                crate::stdio::cirka_fd_close(l6);
            }
            // Resolution: numeric addresses and names from the hosts file.
            let r = cirka_net_resolve(lo);
            assert_eq!(io::bytes_of(r), b"127.0.0.1");
            cirka_drop(r);
            let name = text("localhost");
            let r = cirka_net_resolve(name);
            let got = io::bytes_of(r).to_vec();
            assert!(got == b"127.0.0.1" || got == b"::1", "{got:?}");
            cirka_drop(r);
            let nul = crate::cirka_bytes_from(b"a\0b".as_ptr(), 3);
            let r = cirka_net_resolve(nul);
            assert!(io::bytes_of(r).is_empty());
            assert_eq!(crate::stdio::cirka_errno(), i64::from(ENOENT));
            cirka_drop(r);
            for o in [bad, any, v6, lo, name, nul] {
                cirka_drop(o);
            }
        }
    }

    #[test]
    fn descriptor_helpers() {
        let mut fds = [0i32; 2];
        unsafe extern "C" {
            fn pipe(fds: *mut i32) -> i32;
        }
        // SAFETY: valid out array.
        assert_eq!(unsafe { pipe(fds.as_mut_ptr()) }, 0);
        let r = i64::from(fds[0]);
        assert_eq!(cirka_fd_set_nonblocking(r, 1), 0);
        let d = crate::stdio::cirka_fd_read(r, 8);
        // SAFETY: a fresh string.
        unsafe {
            assert!(io::bytes_of(d).is_empty());
            cirka_drop(d);
        }
        assert_eq!(
            crate::stdio::cirka_errno(),
            11,
            "EAGAIN: nothing to read yet"
        );
        assert_eq!(cirka_fd_set_nonblocking(r, 0), 0);
        assert_eq!(cirka_fd_set_nonblocking(-1, 1), -9);
        crate::stdio::cirka_fd_close(r);
        crate::stdio::cirka_fd_close(i64::from(fds[1]));
        let n = cirka_fd_limit_raise();
        assert!(n >= 1024, "{n}");
        assert_eq!(cirka_fd_limit_raise(), n, "already raised");
    }

    #[test]
    fn descriptor_objects_close_their_descriptor() {
        let _g = LOCK.lock().unwrap();
        // SAFETY: descriptor objects released below.
        unsafe {
            let lo = text("127.0.0.1");
            let fd = cirka_net_listen(lo, 0, 0);
            assert!(fd >= 0);
            let h = cirka_fd_handle(fd);
            assert_eq!(cirka_handle_fd(h), fd);
            // No connection is pending; then freeing the object closes
            // the descriptor.
            assert_eq!(cirka_sock_accept(h), -11);
            cirka_drop(h);
            assert_eq!(cirka_fd_set_nonblocking(fd, 1), -9, "closed");
            // Closed early: freeing the object does not close the number
            // again (it may belong to another descriptor by then).
            let fd = cirka_net_listen(lo, 0, 0);
            let h = cirka_fd_handle(fd);
            assert_eq!(cirka_handle_close(h), 0);
            let again = cirka_net_listen(lo, 0, 0);
            assert_eq!(again, fd, "the number is reused");
            cirka_drop(h);
            assert_eq!(cirka_fd_set_nonblocking(again, 1), 0, "still open");
            crate::stdio::cirka_fd_close(again);
            assert_eq!(cirka_sock_accept(h_closed()), -9);
            cirka_drop(lo);
        }
    }

    /// A descriptor object whose descriptor is closed.
    fn h_closed() -> Obj {
        let mut fds = [0i32; 2];
        unsafe extern "C" {
            fn pipe(fds: *mut i32) -> i32;
        }
        // SAFETY: valid out array; the object is leaked by the caller's
        // use (a test).
        unsafe {
            assert_eq!(pipe(fds.as_mut_ptr()), 0);
            crate::stdio::cirka_fd_close(i64::from(fds[1]));
            let h = cirka_fd_handle(i64::from(fds[0]));
            assert_eq!(cirka_handle_close(h), 0);
            h
        }
    }

    #[test]
    fn datagrams_and_their_sender() {
        let _g = LOCK.lock().unwrap();
        // SAFETY: descriptor objects and strings released below.
        unsafe {
            let lo = text("127.0.0.1");
            let a = cirka_fd_handle(cirka_net_udp_bind(lo, 0));
            let b = cirka_fd_handle(cirka_net_udp_bind(lo, 0));
            let pa = cirka_net_local_port(cirka_handle_fd(a));
            let pb = cirka_net_local_port(cirka_handle_fd(b));
            assert!(pa > 0 && pb > 0 && pa != pb);
            let none = cirka_sock_recvfrom(b, 100);
            assert!(io::bytes_of(none).is_empty());
            assert_eq!(crate::stdio::cirka_errno(), 11, "nothing yet");
            assert_eq!(cirka_net_peer_port(), 0);
            cirka_drop(none);
            let m = text("datagram");
            assert_eq!(cirka_sock_sendto(a, m, lo, pb), 8);
            let empty = text("");
            assert_eq!(cirka_sock_sendto(a, empty, lo, pb), 0);
            let bad = text("nowhere");
            assert_eq!(cirka_sock_sendto(a, m, bad, pb), -EINVAL);
            // Blocking reads would wait: the datagrams are there already
            // on the loopback interface.
            let d = cirka_sock_recvfrom(b, 100);
            assert_eq!(io::bytes_of(d), b"datagram");
            assert_eq!(crate::stdio::cirka_errno(), 0);
            let host = cirka_net_peer_host();
            assert_eq!(io::bytes_of(host), b"127.0.0.1");
            assert_eq!(cirka_net_peer_port(), pa);
            let e = cirka_sock_recvfrom(b, 100);
            assert!(io::bytes_of(e).is_empty());
            assert_eq!(crate::stdio::cirka_errno(), 0, "an empty datagram");
            // Kept to `max` bytes.
            assert_eq!(cirka_sock_sendto(a, m, lo, pb), 8);
            let short = cirka_sock_recvfrom(b, 4);
            assert_eq!(io::bytes_of(short), b"data");
            for o in [d, host, e, short, m, empty, bad, a, b, lo] {
                cirka_drop(o);
            }
            assert_eq!(cirka_net_udp_bind(text("x"), 0), -EINVAL);
        }
    }
}
