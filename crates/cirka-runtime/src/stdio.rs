//! Files, standard streams, the environment and clocks for the CirkaLang
//! standard library (`Std.*`, FEAT-0213).
//!
//! Failures do not end the program: a function returns `-errno` (or an
//! empty string with `errno` set, see `cirka_errno`) and the library turns
//! it into a `Result`. File descriptors are plain integers; the library
//! owns them through a linear `File` value.

use crate::io;
use crate::object::Obj;
use crate::sys;
use core::ffi::{c_char, c_void};

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const O_APPEND: i32 = 0o2000;
const O_CLOEXEC: i32 = 0o2000000;
const EINTR: i32 = 4;

fn errno() -> i32 {
    // SAFETY: the thread's errno location.
    unsafe { *sys::__errno_location() }
}

fn set_errno(e: i32) {
    // SAFETY: the thread's errno location.
    unsafe { *sys::__errno_location() = e }
}

/// `s` as a NUL-terminated C string in a malloc'ed buffer (free it).
///
/// # Safety
/// `s` must be a live BYTES object.
unsafe fn c_string(s: Obj) -> *mut c_char {
    // SAFETY: guaranteed by the caller; the buffer has len + 1 bytes.
    unsafe {
        let p = io::bytes_of(s);
        let buf = sys::malloc(p.len() + 1) as *mut u8;
        if buf.is_null() {
            io::fatal(b"out of memory", 251);
        }
        core::ptr::copy_nonoverlapping(p.as_ptr(), buf, p.len());
        *buf.add(p.len()) = 0;
        buf as *mut c_char
    }
}

/// The last error of this thread (0 after a success).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_errno() -> i64 {
    i64::from(errno())
}

/// Opens `path`: `mode` 0 reads, 1 writes (creating or truncating), 2
/// appends (creating). The descriptor, or `-errno`.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fs_open(path: Obj, mode: i64) -> i64 {
    let flags = O_CLOEXEC
        | match mode {
            0 => O_RDONLY,
            1 => O_WRONLY | O_CREAT | O_TRUNC,
            _ => O_WRONLY | O_CREAT | O_APPEND,
        };
    // SAFETY: valid C string; `path` is live.
    unsafe {
        let cp = c_string(path);
        let fd = loop {
            let fd = sys::open(cp, flags, 0o666 as core::ffi::c_uint);
            if fd >= 0 || errno() != EINTR {
                break fd;
            }
        };
        let e = errno();
        sys::free(cp as *mut c_void);
        if fd < 0 { -i64::from(e) } else { i64::from(fd) }
    }
}

/// Removes the file at `path`: 0, or `-errno`.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fs_remove(path: Obj) -> i64 {
    // SAFETY: valid C string; `path` is live.
    unsafe {
        let cp = c_string(path);
        let r = sys::unlink(cp);
        let e = errno();
        sys::free(cp as *mut c_void);
        if r < 0 { -i64::from(e) } else { 0 }
    }
}

/// Reads up to `max` bytes from `fd` (all of it until the end when `max`
/// is negative). At the end of the input the string is empty and `errno`
/// is 0; after a failure it is empty and `errno` is set. Descriptor 0
/// reads through the stdin buffer shared with `readInt`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_fd_read(fd: i64, max: i64) -> Obj {
    let mut cap = if max < 0 { 4096 } else { (max as usize).max(1) };
    // SAFETY: malloc'ed buffer of `cap` bytes, grown by copying.
    unsafe {
        let mut buf = sys::malloc(cap) as *mut u8;
        if buf.is_null() {
            io::fatal(b"out of memory", 251);
        }
        let mut len = 0usize;
        let mut err = 0;
        loop {
            if max >= 0 && len == max as usize {
                break;
            }
            if len == cap {
                let nb = sys::malloc(cap * 2) as *mut u8;
                if nb.is_null() {
                    io::fatal(b"out of memory", 251);
                }
                core::ptr::copy_nonoverlapping(buf, nb, len);
                sys::free(buf as *mut c_void);
                buf = nb;
                cap *= 2;
            }
            let n = if fd == 0 {
                io::stdin_read(core::slice::from_raw_parts_mut(buf.add(len), cap - len)) as isize
            } else {
                sys::read(fd as i32, buf.add(len) as *mut c_void, cap - len)
            };
            if n < 0 {
                let e = errno();
                if e == EINTR {
                    continue;
                }
                err = e;
                len = 0;
                break;
            }
            if n == 0 {
                break;
            }
            len += n as usize;
            // One read for a bounded request: return what is available.
            if max >= 0 {
                break;
            }
        }
        let o = crate::cirka_bytes_from(buf, len);
        sys::free(buf as *mut c_void);
        set_errno(err);
        o
    }
}

/// Writes all of `data` to `fd`: 0, or `-errno`. Standard output is
/// flushed first so that the order of the output is kept.
///
/// # Safety
/// `data` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fd_write(fd: i64, data: Obj) -> i64 {
    if fd == 1 || fd == 2 {
        io::flush();
    }
    // SAFETY: `data` is live.
    let mut d = unsafe { io::bytes_of(data) };
    while !d.is_empty() {
        // SAFETY: valid buffer.
        let n = unsafe { sys::write(fd as i32, d.as_ptr() as *const c_void, d.len()) };
        if n < 0 {
            let e = errno();
            if e == EINTR {
                continue;
            }
            return -i64::from(e);
        }
        d = &d[n as usize..];
    }
    0
}

/// Closes `fd`: 0, or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_fd_close(fd: i64) -> i64 {
    // SAFETY: closing a descriptor has no memory effects.
    let r = unsafe { sys::close(fd as i32) };
    if r < 0 { -i64::from(errno()) } else { 0 }
}

/// A description of an error number (the same text everywhere, not the
/// C library's localized one).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_strerror(e: i64) -> Obj {
    let text: &[u8] = match e {
        1 => b"Operation not permitted",
        2 => b"No such file or directory",
        4 => b"Interrupted system call",
        5 => b"Input/output error",
        9 => b"Bad file descriptor",
        11 => b"Resource temporarily unavailable",
        12 => b"Cannot allocate memory",
        13 => b"Permission denied",
        17 => b"File exists",
        20 => b"Not a directory",
        21 => b"Is a directory",
        22 => b"Invalid argument",
        24 => b"Too many open files",
        27 => b"File too large",
        28 => b"No space left on device",
        30 => b"Read-only file system",
        32 => b"Broken pipe",
        36 => b"File name too long",
        88 => b"Socket operation on non-socket",
        97 => b"Address family not supported by protocol",
        98 => b"Address already in use",
        99 => b"Cannot assign requested address",
        101 => b"Network is unreachable",
        103 => b"Software caused connection abort",
        104 => b"Connection reset by peer",
        107 => b"Transport endpoint is not connected",
        110 => b"Connection timed out",
        111 => b"Connection refused",
        113 => b"No route to host",
        115 => b"Operation now in progress",
        _ => b"Unknown error",
    };
    // SAFETY: a valid slice.
    unsafe { crate::cirka_bytes_from(text.as_ptr(), text.len()) }
}

/// Whether stdin has no more bytes.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_stdin_eof() -> i64 {
    i64::from(io::cirka_peek_byte() < 0)
}

/// The next line of stdin without its `\n` (and a `\r` before it); empty
/// at the end of the input (test `cirka_stdin_eof` first).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_read_line() -> Obj {
    let mut cap = 128usize;
    // SAFETY: malloc'ed buffer of `cap` bytes, grown by copying.
    unsafe {
        let mut buf = sys::malloc(cap) as *mut u8;
        if buf.is_null() {
            io::fatal(b"out of memory", 251);
        }
        let mut len = 0usize;
        loop {
            let c = io::cirka_read_byte();
            if c < 0 || c == 10 {
                break;
            }
            if len == cap {
                let nb = sys::malloc(cap * 2) as *mut u8;
                if nb.is_null() {
                    io::fatal(b"out of memory", 251);
                }
                core::ptr::copy_nonoverlapping(buf, nb, len);
                sys::free(buf as *mut c_void);
                buf = nb;
                cap *= 2;
            }
            *buf.add(len) = c as u8;
            len += 1;
        }
        if len > 0 && *buf.add(len - 1) == 13 {
            len -= 1;
        }
        let o = crate::cirka_bytes_from(buf, len);
        sys::free(buf as *mut c_void);
        o
    }
}

/// The value of the environment variable `name` (empty when unset; see
/// `cirka_env_has`).
///
/// # Safety
/// `name` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_getenv(name: Obj) -> Obj {
    // SAFETY: valid C strings; getenv's result is read before any change.
    unsafe {
        let cn = c_string(name);
        let v = sys::getenv(cn);
        sys::free(cn as *mut c_void);
        if v.is_null() {
            return crate::cirka_bytes_from(b"".as_ptr(), 0);
        }
        let s = core::ffi::CStr::from_ptr(v).to_bytes();
        crate::cirka_bytes_from(s.as_ptr(), s.len())
    }
}

/// Whether the environment variable `name` is set.
///
/// # Safety
/// `name` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_env_has(name: Obj) -> i64 {
    // SAFETY: valid C string.
    unsafe {
        let cn = c_string(name);
        let v = sys::getenv(cn);
        sys::free(cn as *mut c_void);
        i64::from(!v.is_null())
    }
}

/// Nanoseconds of a clock: 0 the wall clock (since the Unix epoch), 1 a
/// monotonic clock (since an unspecified point).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_clock_ns(clock: i64) -> i64 {
    let mut ts = sys::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let id = if clock == 0 { 0 } else { 1 };
    // SAFETY: a valid timespec.
    unsafe { sys::clock_gettime(id, &mut ts) };
    ts.tv_sec * 1_000_000_000 + ts.tv_nsec
}

/// A NUL-terminated copy of `s` in C memory (freed with `free`), as an
/// address (`withCString`).
///
/// # Safety
/// `s` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_cstring_new(s: Obj) -> i64 {
    // SAFETY: guaranteed by the caller.
    unsafe { c_string(s) as i64 }
}

/// The `n` bytes at address `p` as a string (`peekCString`).
///
/// # Safety
/// `p` must point to `n` readable bytes (unchecked, as in C).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_cstring_peek(p: i64, n: i64) -> Obj {
    if n <= 0 {
        // SAFETY: an empty slice.
        return unsafe { crate::cirka_bytes_from(b"".as_ptr(), 0) };
    }
    // SAFETY: guaranteed by the caller.
    unsafe { crate::cirka_bytes_from(p as *const u8, n as usize) }
}
