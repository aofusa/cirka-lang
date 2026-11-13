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

/// Formats a double like `cirka_write_f64` into a new BYTES object.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_f64_to_bytes(x: f64) -> Obj {
    let mut buf = [0u8; 64];
    let mut f = Fmt {
        buf: &mut buf,
        len: 0,
    };
    let _ = core::fmt::write(&mut f, format_args!("{x}"));
    let n = f.len;
    // SAFETY: `buf` holds `n` initialized bytes.
    unsafe { crate::cirka_bytes_from(buf.as_ptr(), n) }
}

/// `show` of a `Double` in Haskell's format (GHC's `showFloat`): the
/// shortest digits that round-trip, in fixed notation when the decimal
/// exponent is in 0..=7 (`0.1`, `1234567.0`) and otherwise in scientific
/// notation (`1.0e-2`, `1.0e7`).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_f64_show_hs(x: f64) -> Obj {
    let mut out = [0u8; 64];
    let n = show_hs(x, false, &mut out);
    // SAFETY: `out` holds `n` initialized bytes.
    unsafe { crate::cirka_bytes_from(out.as_ptr(), n) }
}

/// `show` of a `Float` (a double holding a single-precision value): the
/// shortest digits identifying the single-precision number.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_f32_show_hs(x: f64) -> Obj {
    let mut out = [0u8; 64];
    let n = show_hs(x, true, &mut out);
    // SAFETY: `out` holds `n` initialized bytes.
    unsafe { crate::cirka_bytes_from(out.as_ptr(), n) }
}

/// `x` rounded to single precision (`Float` arithmetic).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_f32_round(x: f64) -> f64 {
    x as f32 as f64
}

/// The IEEE bits of `x` (`decodeFloat`, `isNegativeZero` …).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_f64_bits(x: f64) -> i64 {
    x.to_bits() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn cirka_f64_from_bits(b: i64) -> f64 {
    f64::from_bits(b as u64)
}

/// Parses a decimal floating-point literal (correctly rounded); NaN when
/// it is not one.
///
/// # Safety
/// `o` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_parse_f64(o: Obj) -> f64 {
    // SAFETY: guaranteed by the caller.
    let b = unsafe { bytes_of(o) };
    core::str::from_utf8(b)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(f64::NAN)
}

fn show_hs(x: f64, single: bool, out: &mut [u8; 64]) -> usize {
    let mut n = 0;
    let mut push = |out: &mut [u8; 64], s: &[u8]| {
        let k = s.len().min(64 - n);
        out[n..n + k].copy_from_slice(&s[..k]);
        n += k;
    };
    if x.is_nan() {
        push(out, b"NaN");
        return n;
    }
    if x.is_sign_negative() {
        push(out, b"-");
    }
    if x.is_infinite() {
        push(out, b"Infinity");
        return n;
    }
    if x == 0.0 {
        push(out, b"0.0");
        return n;
    }
    // Shortest digits `d.ddd` and exponent from `{:e}`.
    let mut buf = [0u8; 64];
    let mut f = Fmt {
        buf: &mut buf,
        len: 0,
    };
    if single {
        let _ = core::fmt::write(&mut f, format_args!("{:e}", x.abs() as f32));
    } else {
        let _ = core::fmt::write(&mut f, format_args!("{:e}", x.abs()));
    }
    let len = f.len;
    let s = &buf[..len];
    let epos = s.iter().position(|&c| c == b'e').unwrap_or(len);
    let mut digits = [0u8; 32];
    let mut nd = 0;
    for &c in &s[..epos] {
        if c.is_ascii_digit() && nd < 32 {
            digits[nd] = c;
            nd += 1;
        }
    }
    let mut exp: i32 = 0;
    let mut neg = false;
    for &c in &s[(epos + 1).min(len)..] {
        match c {
            b'-' => neg = true,
            b'0'..=b'9' => exp = exp * 10 + (c - b'0') as i32,
            _ => {}
        }
    }
    if neg {
        exp = -exp;
    }
    // x = 0.d1d2.. * 10^e
    let e = exp + 1;
    let digits = &digits[..nd];
    if (0..=7).contains(&e) {
        let e = e as usize;
        if e == 0 {
            push(out, b"0.");
            push(out, digits);
        } else {
            for i in 0..e {
                push(out, &[*digits.get(i).unwrap_or(&b'0')]);
            }
            push(out, b".");
            if nd > e {
                push(out, &digits[e..]);
            } else {
                push(out, b"0");
            }
        }
    } else {
        push(out, &digits[..1]);
        push(out, b".");
        if nd > 1 {
            push(out, &digits[1..]);
        } else {
            push(out, b"0");
        }
        push(out, b"e");
        let mut eb = [0u8; 12];
        let mut f = Fmt {
            buf: &mut [0u8; 64],
            len: 0,
        };
        let _ = core::fmt::write(&mut f, format_args!("{}", e - 1));
        let k = f.len;
        eb[..k].copy_from_slice(&f.buf[..k]);
        push(out, &eb[..k]);
    }
    n
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

/// Reads from stdin into `out` through the buffer shared with
/// `cirka_read_byte`: the number of bytes (0 at the end of input), or -1
/// with `errno` set.
pub fn stdin_read(out: &mut [u8]) -> isize {
    // SAFETY: stdin buffer used from one thread.
    let b = unsafe { STDIN.get() };
    if b.pos < b.len {
        let n = (b.len - b.pos).min(out.len());
        out[..n].copy_from_slice(&b.data[b.pos..b.pos + n]);
        b.pos += n;
        return n as isize;
    }
    if b.eof {
        return 0;
    }
    flush();
    // SAFETY: valid buffer.
    let n = unsafe { sys::read(0, out.as_mut_ptr() as *mut c_void, out.len()) };
    if n == 0 {
        b.eof = true;
    }
    n
}

/// The next byte of stdin without consuming it; -1 at end of input.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_peek_byte() -> i64 {
    let c = cirka_read_byte();
    if c >= 0 {
        // SAFETY: stdin buffer used from one thread; the byte just read is
        // still in the buffer.
        unsafe { STDIN.get().pos -= 1 };
    }
    c
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
