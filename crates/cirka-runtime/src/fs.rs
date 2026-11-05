//! Whole-file reading and writing (`readFile` / `writeFile` /
//! `appendFile`). Errors end the program with GHC's message format until
//! exceptions can be caught (FEAT-0040).

use crate::io;
use crate::object::Obj;
use crate::sys;
use core::ffi::{c_char, c_void};

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const O_APPEND: i32 = 0o2000;

/// `path` as a NUL-terminated C string in a malloc'ed buffer.
///
/// # Safety
/// `path` must be a live BYTES object.
unsafe fn c_path(path: Obj) -> *mut c_char {
    // SAFETY: guaranteed by the caller; the buffer has len + 1 bytes.
    unsafe {
        let p = io::bytes_of(path);
        let buf = sys::malloc(p.len() + 1) as *mut u8;
        if buf.is_null() {
            io::fatal(b"out of memory", 251);
        }
        core::ptr::copy_nonoverlapping(p.as_ptr(), buf, p.len());
        *buf.add(p.len()) = 0;
        buf as *mut c_char
    }
}

/// Ends the program like GHC's uncaught `IOError`:
/// `prog: PATH: OP: DESCRIPTION (STRERROR)`.
///
/// # Safety
/// `path` must be a live BYTES object.
unsafe fn io_error(path: Obj, op: &[u8], errno: i32) -> ! {
    let (kind, text): (&[u8], &[u8]) = match errno {
        2 => (b"does not exist", b"No such file or directory"),
        13 => (b"permission denied", b"Permission denied"),
        21 => (b"inappropriate type", b"is a directory"),
        20 => (b"does not exist", b"Not a directory"),
        28 => (b"resource exhausted", b"No space left on device"),
        _ => (b"failed", b"I/O error"),
    };
    let mut msg = [0u8; 512];
    let mut n = 0;
    // SAFETY: guaranteed by the caller.
    let p = unsafe { io::bytes_of(path) };
    for part in [p, b": ", op, b": ", kind, b" (", text, b")"] {
        let k = part.len().min(msg.len() - n);
        msg[n..n + k].copy_from_slice(&part[..k]);
        n += k;
    }
    crate::start::panic_msg(&msg[..n])
}

fn errno() -> i32 {
    // SAFETY: the thread's errno location.
    unsafe { *sys::__errno_location() }
}

/// The contents of the file at `path` as a fresh BYTES object.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_file_read(path: Obj) -> Obj {
    // SAFETY: valid C string and buffers; `path` is live.
    unsafe {
        let cp = c_path(path);
        let fd = sys::open(cp, O_RDONLY, 0);
        sys::free(cp as *mut c_void);
        if fd < 0 {
            io_error(path, b"openFile", errno());
        }
        let mut cap = 4096usize;
        let mut len = 0usize;
        let mut buf = sys::malloc(cap) as *mut u8;
        loop {
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
            let n = sys::read(fd, buf.add(len) as *mut c_void, cap - len);
            if n < 0 {
                let e = errno();
                if e == 4 {
                    continue;
                }
                sys::close(fd);
                // A directory opens but cannot be read; GHC fails in openFile.
                io_error(
                    path,
                    if e == 21 {
                        b"openFile"
                    } else {
                        b"hGetContents"
                    },
                    e,
                );
            }
            if n == 0 {
                break;
            }
            len += n as usize;
        }
        sys::close(fd);
        let o = crate::cirka_bytes_from(buf, len);
        sys::free(buf as *mut c_void);
        o
    }
}

/// Writes `data` to the file at `path` (truncating, or appending when
/// `append != 0`).
///
/// # Safety
/// `path` and `data` must be live BYTES objects (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_file_write(path: Obj, data: Obj, append: i64) -> i64 {
    // SAFETY: valid C string and buffers; both objects are live.
    unsafe {
        let flags = O_WRONLY | O_CREAT | if append != 0 { O_APPEND } else { O_TRUNC };
        let cp = c_path(path);
        let fd = sys::open(cp, flags, 0o666 as core::ffi::c_uint);
        sys::free(cp as *mut c_void);
        if fd < 0 {
            io_error(path, b"withFile", errno());
        }
        let mut d = io::bytes_of(data);
        while !d.is_empty() {
            let n = sys::write(fd, d.as_ptr() as *const c_void, d.len());
            if n < 0 {
                let e = errno();
                if e == 4 {
                    continue;
                }
                sys::close(fd);
                io_error(path, b"withFile", e);
            }
            d = &d[n as usize..];
        }
        sys::close(fd);
    }
    0
}
