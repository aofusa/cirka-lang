//! cirka-bigint: arbitrary-precision integers for Haskell's `Integer`
//! (FEAT-0114, 05-runtime.md §5), shared by the runtime (`no_std`, no
//! allocator) and the MIR interpreter.
//!
//! Nothing allocates: callers pass output and scratch slices, sized with
//! the `*_len` functions. A magnitude is a little-endian slice of `u32`
//! limbs without high zero limbs (zero is the empty slice). The *encoded*
//! form stored in a runtime `BYTES` object is one sign byte (0 or 1)
//! followed by the limbs as 4-byte little-endian words.
#![no_std]

use core::cmp::Ordering;

/// A signed integer viewed as sign and magnitude.
#[derive(Clone, Copy, Debug)]
pub struct Int<'a> {
    pub neg: bool,
    pub mag: &'a [u32],
}

/// The magnitude without high zero limbs.
pub fn trim(a: &[u32]) -> &[u32] {
    let mut n = a.len();
    while n > 0 && a[n - 1] == 0 {
        n -= 1;
    }
    &a[..n]
}

fn trimmed_len(a: &[u32]) -> usize {
    trim(a).len()
}

// ---- encoding ----

/// Bytes needed to encode a magnitude of `n` limbs.
pub fn encoded_len(n: usize) -> usize {
    1 + 4 * n
}

/// Writes the encoded form of `neg`/`mag` into `out` (at least
/// `encoded_len(mag.len())` bytes); returns the number of bytes written.
/// Zero is never negative.
pub fn encode(neg: bool, mag: &[u32], out: &mut [u8]) -> usize {
    let mag = trim(mag);
    out[0] = u8::from(neg && !mag.is_empty());
    for (i, l) in mag.iter().enumerate() {
        out[1 + 4 * i..5 + 4 * i].copy_from_slice(&l.to_le_bytes());
    }
    encoded_len(mag.len())
}

/// Number of limbs of an encoded integer.
pub fn decoded_len(bytes: &[u8]) -> usize {
    bytes.len().saturating_sub(1) / 4
}

/// Decodes into `out` (at least `decoded_len` limbs); returns the sign and
/// the number of limbs.
pub fn decode(bytes: &[u8], out: &mut [u32]) -> (bool, usize) {
    let n = decoded_len(bytes);
    for i in 0..n {
        let mut w = [0u8; 4];
        w.copy_from_slice(&bytes[1 + 4 * i..5 + 4 * i]);
        out[i] = u32::from_le_bytes(w);
    }
    let n = trimmed_len(&out[..n]);
    (bytes.first() == Some(&1) && n > 0, n)
}

// ---- conversions ----

/// The magnitude of `x` (2 limbs).
pub fn from_i64(x: i64, out: &mut [u32]) -> (bool, usize) {
    let m = x.unsigned_abs();
    out[0] = m as u32;
    out[1] = (m >> 32) as u32;
    (x < 0, trimmed_len(&out[..2]))
}

/// Whether the integer fits in an `i64`.
pub fn fits_i64(a: Int) -> bool {
    match a.mag.len() {
        0 | 1 => true,
        2 => {
            let m = u64::from(a.mag[0]) | (u64::from(a.mag[1]) << 32);
            if a.neg { m <= 1 << 63 } else { m < 1 << 63 }
        }
        _ => false,
    }
}

/// The low 64 bits as an `i64` (two's complement wrap, like GHC's
/// `fromInteger :: Integer -> Int`).
pub fn to_i64_wrapping(a: Int) -> i64 {
    let lo = u64::from(a.mag.first().copied().unwrap_or(0))
        | (u64::from(a.mag.get(1).copied().unwrap_or(0)) << 32);
    if a.neg {
        (lo as i64).wrapping_neg()
    } else {
        lo as i64
    }
}

fn bit_len(a: &[u32]) -> usize {
    match a.last() {
        None => 0,
        Some(h) => 32 * (a.len() - 1) + (32 - h.leading_zeros() as usize),
    }
}

/// The nearest `f64` (ties to even; infinite when too large).
pub fn to_f64(a: Int) -> f64 {
    let bits = bit_len(a.mag);
    let m = if bits <= 64 {
        let lo = u64::from(a.mag.first().copied().unwrap_or(0))
            | (u64::from(a.mag.get(1).copied().unwrap_or(0)) << 32);
        lo as f64
    } else {
        // The top 64 bits with a sticky bit for the rest: rounding them to
        // 53 bits gives the correctly rounded result.
        let shift = bits - 64;
        let mut top = 0u64;
        for i in 0..64 {
            let b = shift + i;
            if (a.mag[b / 32] >> (b % 32)) & 1 == 1 {
                top |= 1 << i;
            }
        }
        let sticky = (0..shift).any(|b| (a.mag[b / 32] >> (b % 32)) & 1 == 1);
        let x = (top | u64::from(sticky)) as f64;
        let mut s = shift;
        let mut x = x;
        while s > 0 && x.is_finite() {
            let k = s.min(1000);
            x *= pow2(k as i32);
            s -= k;
        }
        x
    };
    if a.neg { -m } else { m }
}

fn pow2(k: i32) -> f64 {
    // 2^k for 0 <= k <= 1000 by repeated squaring of exact powers.
    let mut r = 1.0f64;
    let mut b = 2.0f64;
    let mut k = k;
    while k > 0 {
        if k & 1 == 1 {
            r *= b;
        }
        b *= b;
        k >>= 1;
    }
    r
}

/// Limbs needed for [`from_f64`].
pub const F64_LIMBS: usize = 34;

/// `x` truncated toward zero (`truncate`); NaN and infinities give 0.
pub fn from_f64(x: f64, out: &mut [u32]) -> (bool, usize) {
    for l in out.iter_mut().take(F64_LIMBS) {
        *l = 0;
    }
    if !x.is_finite() {
        return (false, 0);
    }
    let bits = x.to_bits();
    let neg = bits >> 63 == 1;
    let exp = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1 << 52) - 1);
    if exp == 0 {
        return (false, 0); // subnormal: |x| < 1
    }
    let m = frac | (1 << 52);
    let e = exp - 1075; // x = m * 2^e
    if e <= -53 {
        return (false, 0);
    }
    if e < 0 {
        let v = m >> (-e);
        out[0] = v as u32;
        out[1] = (v >> 32) as u32;
    } else {
        let e = e as usize;
        for i in 0..53 {
            if (m >> i) & 1 == 1 {
                let b = e + i;
                out[b / 32] |= 1 << (b % 32);
            }
        }
    }
    let n = trimmed_len(&out[..F64_LIMBS]);
    (neg && n > 0, n)
}

// ---- magnitudes ----

/// Compares magnitudes.
pub fn cmp_mag(a: &[u32], b: &[u32]) -> Ordering {
    let (a, b) = (trim(a), trim(b));
    a.len().cmp(&b.len()).then_with(|| {
        for i in (0..a.len()).rev() {
            match a[i].cmp(&b[i]) {
                Ordering::Equal => {}
                o => return o,
            }
        }
        Ordering::Equal
    })
}

/// Compares signed integers.
pub fn cmp(a: Int, b: Int) -> Ordering {
    let an = a.neg && !trim(a.mag).is_empty();
    let bn = b.neg && !trim(b.mag).is_empty();
    match (an, bn) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        (false, false) => cmp_mag(a.mag, b.mag),
        (true, true) => cmp_mag(b.mag, a.mag),
    }
}

/// `a + b` (`out.len() > max(a.len(), b.len())`); returns the length.
pub fn add_mag(a: &[u32], b: &[u32], out: &mut [u32]) -> usize {
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut carry = 0u64;
    for i in 0..a.len() {
        let s = u64::from(a[i]) + u64::from(b.get(i).copied().unwrap_or(0)) + carry;
        out[i] = s as u32;
        carry = s >> 32;
    }
    out[a.len()] = carry as u32;
    trimmed_len(&out[..a.len() + 1])
}

/// `a - b` for `a >= b` (`out.len() >= a.len()`); returns the length.
pub fn sub_mag(a: &[u32], b: &[u32], out: &mut [u32]) -> usize {
    let mut borrow = 0i64;
    for i in 0..a.len() {
        let d = i64::from(a[i]) - i64::from(b.get(i).copied().unwrap_or(0)) - borrow;
        if d < 0 {
            out[i] = (d + (1 << 32)) as u32;
            borrow = 1;
        } else {
            out[i] = d as u32;
            borrow = 0;
        }
    }
    trimmed_len(&out[..a.len()])
}

/// `a * b` (`out.len() >= a.len() + b.len()`); returns the length.
pub fn mul_mag(a: &[u32], b: &[u32], out: &mut [u32]) -> usize {
    let n = a.len() + b.len();
    for o in out.iter_mut().take(n) {
        *o = 0;
    }
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = u64::from(x) * u64::from(y) + u64::from(out[i + j]) + carry;
            out[i + j] = t as u32;
            carry = t >> 32;
        }
        out[i + b.len()] = carry as u32;
    }
    trimmed_len(&out[..n])
}

/// `a / d` and `a % d` for a one-limb `d > 0` (`q.len() >= a.len()`).
pub fn divrem_small(a: &[u32], d: u32, q: &mut [u32]) -> (usize, u32) {
    let mut r = 0u64;
    for i in (0..a.len()).rev() {
        let cur = (r << 32) | u64::from(a[i]);
        q[i] = (cur / u64::from(d)) as u32;
        r = cur % u64::from(d);
    }
    (trimmed_len(&q[..a.len()]), r as u32)
}

/// Scratch limbs needed by [`divrem_mag`] / [`quot_rem`].
pub fn divrem_scratch_len(alen: usize, blen: usize) -> usize {
    alen + 1 + blen
}

/// `u / v` and `u % v` for `v != 0` (Knuth, TAOCP vol. 2, 4.3.1 D).
/// `q.len() >= u.len() + 1`, `r.len() >= v.len()`, and `scratch` as
/// [`divrem_scratch_len`]. Returns the lengths of `q` and `r`.
pub fn divrem_mag(
    u: &[u32],
    v: &[u32],
    q: &mut [u32],
    r: &mut [u32],
    scratch: &mut [u32],
) -> (usize, usize) {
    let (u, v) = (trim(u), trim(v));
    assert!(!v.is_empty(), "division by zero");
    if cmp_mag(u, v) == Ordering::Less {
        r[..u.len()].copy_from_slice(u);
        return (0, u.len());
    }
    if v.len() == 1 {
        let (ql, rem) = divrem_small(u, v[0], q);
        r[0] = rem;
        return (ql, usize::from(rem != 0));
    }
    let n = v.len();
    let m = u.len() - n;
    let (un, rest) = scratch.split_at_mut(u.len() + 1);
    let vn = &mut rest[..n];
    // D1: normalize so that the top limb of v has its high bit set.
    let s = v[n - 1].leading_zeros();
    for i in (1..n).rev() {
        vn[i] = shl(v[i], s) | shr(v[i - 1], 32 - s);
    }
    vn[0] = shl(v[0], s);
    un[u.len()] = shr(u[u.len() - 1], 32 - s);
    for i in (1..u.len()).rev() {
        un[i] = shl(u[i], s) | shr(u[i - 1], 32 - s);
    }
    un[0] = shl(u[0], s);
    let b = 1u64 << 32;
    for j in (0..=m).rev() {
        // D3: estimate the quotient digit.
        let num = (u64::from(un[j + n]) << 32) | u64::from(un[j + n - 1]);
        let mut qhat = num / u64::from(vn[n - 1]);
        let mut rhat = num % u64::from(vn[n - 1]);
        while qhat >= b || qhat * u64::from(vn[n - 2]) > ((rhat << 32) | u64::from(un[j + n - 2])) {
            qhat -= 1;
            rhat += u64::from(vn[n - 1]);
            if rhat >= b {
                break;
            }
        }
        // D4: multiply and subtract.
        let mut borrow = 0i64;
        let mut carry = 0u64;
        for i in 0..n {
            let p = qhat * u64::from(vn[i]) + carry;
            carry = p >> 32;
            let t = i64::from(un[i + j]) - borrow - (p & 0xffff_ffff) as i64;
            un[i + j] = t as u32;
            borrow = if t < 0 { 1 } else { 0 };
        }
        let t = i64::from(un[j + n]) - borrow - carry as i64;
        un[j + n] = t as u32;
        // D5/D6: add back when the estimate was one too large.
        if t < 0 {
            qhat -= 1;
            let mut c = 0u64;
            for i in 0..n {
                let s = u64::from(un[i + j]) + u64::from(vn[i]) + c;
                un[i + j] = s as u32;
                c = s >> 32;
            }
            un[j + n] = un[j + n].wrapping_add(c as u32);
        }
        q[j] = qhat as u32;
    }
    // D8: unnormalize the remainder.
    for i in 0..n {
        r[i] = shr(un[i], s) | shl(un[i + 1], 32 - s);
    }
    (trimmed_len(&q[..=m]), trimmed_len(&r[..n]))
}

fn shl(x: u32, s: u32) -> u32 {
    if s >= 32 { 0 } else { x << s }
}

fn shr(x: u32, s: u32) -> u32 {
    if s >= 32 { 0 } else { x >> s }
}

// ---- signed operations ----

/// Output limbs needed for `a + b` / `a - b`.
pub fn add_len(a: usize, b: usize) -> usize {
    a.max(b) + 1
}

/// `a + b`; `out.len() >= add_len`.
pub fn add(a: Int, b: Int, out: &mut [u32]) -> (bool, usize) {
    if a.neg == b.neg {
        return (a.neg, add_mag(a.mag, b.mag, out));
    }
    match cmp_mag(a.mag, b.mag) {
        Ordering::Equal => (false, 0),
        Ordering::Greater => {
            let n = sub_mag(trim(a.mag), b.mag, out);
            (a.neg && n > 0, n)
        }
        Ordering::Less => {
            let n = sub_mag(trim(b.mag), a.mag, out);
            (b.neg && n > 0, n)
        }
    }
}

/// `a - b`; `out.len() >= add_len`.
pub fn sub(a: Int, b: Int, out: &mut [u32]) -> (bool, usize) {
    add(
        a,
        Int {
            neg: !b.neg,
            mag: b.mag,
        },
        out,
    )
}

/// `a * b`; `out.len() >= a.mag.len() + b.mag.len()`.
pub fn mul(a: Int, b: Int, out: &mut [u32]) -> (bool, usize) {
    let n = mul_mag(trim(a.mag), trim(b.mag), out);
    (a.neg != b.neg && n > 0, n)
}

/// Truncating division (`quot`, `rem`) for `b != 0`: `q.len() >=
/// a.len() + 1`, `r.len() >= b.len()`, `scratch` as
/// [`divrem_scratch_len`].
pub fn quot_rem(
    a: Int,
    b: Int,
    q: &mut [u32],
    r: &mut [u32],
    scratch: &mut [u32],
) -> ((bool, usize), (bool, usize)) {
    let (ql, rl) = divrem_mag(a.mag, b.mag, q, r, scratch);
    ((a.neg != b.neg && ql > 0, ql), (a.neg && rl > 0, rl))
}

/// Bytes needed for [`to_decimal`] (sign and digits).
pub fn decimal_len(n: usize) -> usize {
    2 + n * 10
}

/// Writes the decimal digits (with `-` for negatives) into `out`
/// (`decimal_len` bytes) using `tmp` (`a.mag.len()` limbs); returns the
/// number of bytes.
pub fn to_decimal(a: Int, tmp: &mut [u32], out: &mut [u8]) -> usize {
    let mag = trim(a.mag);
    if mag.is_empty() {
        out[0] = b'0';
        return 1;
    }
    // Digits are produced backwards at the end of `out`.
    let end = out.len();
    let mut p = end;
    tmp[..mag.len()].copy_from_slice(mag);
    let mut len = mag.len();
    while len > 0 {
        // Divide tmp by 10^9 in place.
        let mut r = 0u64;
        for i in (0..len).rev() {
            let cur = (r << 32) | u64::from(tmp[i]);
            tmp[i] = (cur / 1_000_000_000) as u32;
            r = cur % 1_000_000_000;
        }
        len = trimmed_len(&tmp[..len]);
        let mut chunk = r as u32;
        for k in 0..9 {
            if len == 0 && chunk == 0 && k > 0 {
                break;
            }
            p -= 1;
            out[p] = b'0' + (chunk % 10) as u8;
            chunk /= 10;
        }
    }
    let mut n = 0;
    if a.neg {
        out[0] = b'-';
        n = 1;
    }
    out.copy_within(p..end, n);
    n + (end - p)
}

/// Limbs needed for [`from_decimal`] of `digits` digits.
pub fn from_decimal_len(digits: usize) -> usize {
    digits / 9 + 2
}

/// Parses decimal digits (no sign) into `out` (`from_decimal_len`
/// limbs); `None` when a byte is not a digit.
pub fn from_decimal(digits: &[u8], out: &mut [u32]) -> Option<usize> {
    let mut len = 0usize;
    for chunk in digits.chunks(9) {
        let mut v = 0u32;
        let mut scale = 1u64;
        for &c in chunk {
            if !c.is_ascii_digit() {
                return None;
            }
            v = v * 10 + u32::from(c - b'0');
            scale *= 10;
        }
        // out = out * scale + v
        let mut carry = u64::from(v);
        for l in out.iter_mut().take(len) {
            let t = u64::from(*l) * scale + carry;
            *l = t as u32;
            carry = t >> 32;
        }
        if carry > 0 {
            out[len] = carry as u32;
            len += 1;
        }
    }
    Some(trimmed_len(&out[..len]))
}

#[cfg(test)]
mod tests;
