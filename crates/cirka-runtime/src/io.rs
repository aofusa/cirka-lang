//! Buffered standard I/O used by generated code (05-runtime.md §5).

use crate::Global;
use crate::object::{Obj, word};
use crate::sys;
use core::ffi::c_void;

const BUF: usize = 1 << 16;

struct OutBuf {
    len: usize,
    data: [u8; BUF],
}

static STDOUT: Global<OutBuf> = Global::new(OutBuf {
    len: 0,
    data: [0; BUF],
});

struct InBuf {
    pos: usize,
    len: usize,
    eof: bool,
    data: [u8; BUF],
}

static STDIN: Global<InBuf> = Global::new(InBuf {
    pos: 0,
    len: 0,
    eof: false,
    data: [0; BUF],
});

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        // SAFETY: valid buffer.
        let n = unsafe { sys::write(fd, bytes.as_ptr() as *const c_void, bytes.len()) };
        if n <= 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
}

/// Appends bytes to the stdout buffer.
pub fn out(bytes: &[u8]) {
    // SAFETY: stdout is used from one thread at a time (FEAT-0045 adds locking).
    let b = unsafe { STDOUT.get() };
    if b.len + bytes.len() > BUF {
        flush();
        if bytes.len() > BUF {
            write_all(1, bytes);
            return;
        }
    }
    b.data[b.len..b.len + bytes.len()].copy_from_slice(bytes);
    b.len += bytes.len();
}

pub fn flush() {
    // SAFETY: see `out`.
    let b = unsafe { STDOUT.get() };
    write_all(1, &b.data[..b.len]);
    b.len = 0;
}

/// Unbuffered write to stderr (flushes stdout first to keep ordering).
pub fn ewrite(bytes: &[u8]) {
    flush();
    write_all(2, bytes);
}

pub fn ewrite_u64(n: u64) {
    let mut buf = [0u8; 20];
    ewrite(fmt_u64(n, &mut buf));
}

fn fmt_u64(mut n: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    &buf[i..]
}

/// Prints `msg` to stderr and exits with `code` (after flushing stdout).
pub fn fatal(msg: &[u8], code: i32) -> ! {
    ewrite(msg);
    write_all(2, b"\n");
    // SAFETY: terminating the process.
    unsafe { sys::_exit(code) }
}

#[unsafe(no_mangle)]
pub extern "C" fn cirka_write_i64(n: i64) {
    let mut buf = [0u8; 20];
    if n < 0 {
        out(b"-");
    }
    out(fmt_u64(n.unsigned_abs(), &mut buf));
}

struct Fmt<'a> {
    buf: &'a mut [u8; 64],
    len: usize,
}

impl core::fmt::Write for Fmt<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Writes a double using the shortest representation that round-trips
/// (identical to the MIR interpreter's formatting).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_write_f64(x: f64) {
    let mut buf = [0u8; 64];
    let mut f = Fmt {
        buf: &mut buf,
        len: 0,
    };
    let _ = core::fmt::write(&mut f, format_args!("{x}"));
    let n = f.len;
    out(&buf[..n]);
}

/// Writes a Unicode scalar value as UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_write_char(c: i64) {
    let c = char::from_u32(c as u32).unwrap_or('\u{fffd}');
    let mut buf = [0u8; 4];
    out(c.encode_utf8(&mut buf).as_bytes());
}

/// Bytes of a `BYTES` object (length in word 0, data from word 1).
///
/// # Safety
/// `o` must be a live BYTES object.
pub unsafe fn bytes_of<'a>(o: Obj) -> &'a [u8] {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let len = *word(o, 0);
        core::slice::from_raw_parts(word(o, 1) as *const u8, len)
    }
}

/// # Safety
/// `o` must be a live BYTES object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_write_bytes(o: Obj) {
    // SAFETY: guaranteed by the caller.
    out(unsafe { bytes_of(o) });
}

/// # Safety
/// `o` must be a live BYTES object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_ewrite_bytes(o: Obj) {
    // SAFETY: guaranteed by the caller.
    ewrite(unsafe { bytes_of(o) });
}

#[unsafe(no_mangle)]
pub extern "C" fn cirka_flush() {
    flush();
}

/// Reads one byte from stdin; -1 at end of input.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_read_byte() -> i64 {
    // SAFETY: stdin buffer used from one thread.
    let b = unsafe { STDIN.get() };
    if b.pos == b.len {
        if b.eof {
            return -1;
        }
        flush();
        // SAFETY: valid buffer.
        let n = unsafe { sys::read(0, b.data.as_mut_ptr() as *mut c_void, BUF) };
        if n <= 0 {
            b.eof = true;
            return -1;
        }
        b.pos = 0;
        b.len = n as usize;
    }
    let c = b.data[b.pos];
    b.pos += 1;
    c as i64
}
