extern crate std;
use super::*;
use std::string::String;
use std::vec;
use std::vec::Vec;

fn from_i128(x: i128) -> (bool, Vec<u32>) {
    let mut m = x.unsigned_abs();
    let mut v = Vec::new();
    while m > 0 {
        v.push(m as u32);
        m >>= 32;
    }
    (x < 0, v)
}

fn to_i128(neg: bool, mag: &[u32]) -> i128 {
    let mut m: u128 = 0;
    for (i, l) in mag.iter().enumerate() {
        m |= u128::from(*l) << (32 * i);
    }
    if neg { -(m as i128) } else { m as i128 }
}

fn int(v: &(bool, Vec<u32>)) -> Int<'_> {
    Int {
        neg: v.0,
        mag: &v.1,
    }
}

fn samples() -> Vec<i128> {
    let mut v = vec![
        0,
        1,
        -1,
        7,
        -7,
        u32::MAX as i128,
        u32::MAX as i128 + 1,
        i64::MAX as i128,
        i64::MIN as i128,
        u64::MAX as i128,
        (u64::MAX as i128) * 3 + 12345,
        -((1i128 << 95) + 987654321),
        (1i128 << 100) - 1,
        123456789012345678901234567,
    ];
    let mut x: u64 = 0x9e3779b97f4a7c15;
    for _ in 0..40 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let hi = (x >> 5) as i128;
        v.push(((hi << 40) ^ x as i128) * if x & 1 == 0 { 1 } else { -1 });
    }
    v
}

#[test]
fn add_sub_mul_match_i128() {
    for &a in &samples() {
        for &b in &samples() {
            let (x, y) = (from_i128(a), from_i128(b));
            let mut out = vec![0u32; 8];
            if let Some(s) = a.checked_add(b) {
                let (n, l) = add(int(&x), int(&y), &mut out);
                assert_eq!(to_i128(n, &out[..l]), s, "{a} + {b}");
            }
            if let Some(s) = a.checked_sub(b) {
                let (n, l) = sub(int(&x), int(&y), &mut out);
                assert_eq!(to_i128(n, &out[..l]), s, "{a} - {b}");
            }
            if let Some(p) = a.checked_mul(b) {
                let (n, l) = mul(int(&x), int(&y), &mut out);
                assert_eq!(to_i128(n, &out[..l]), p, "{a} * {b}");
            }
            assert_eq!(cmp(int(&x), int(&y)), a.cmp(&b), "cmp {a} {b}");
        }
    }
}

#[test]
fn quot_rem_matches_i128() {
    for &a in &samples() {
        for &b in &samples() {
            if b == 0 {
                continue;
            }
            let (x, y) = (from_i128(a), from_i128(b));
            let mut q = vec![0u32; x.1.len() + 1];
            let mut r = vec![0u32; y.1.len().max(1)];
            let mut s = vec![0u32; divrem_scratch_len(x.1.len(), y.1.len())];
            let ((qn, ql), (rn, rl)) = quot_rem(int(&x), int(&y), &mut q, &mut r, &mut s);
            assert_eq!(to_i128(qn, &q[..ql]), a / b, "{a} quot {b}");
            assert_eq!(to_i128(rn, &r[..rl]), a % b, "{a} rem {b}");
        }
    }
}

#[test]
fn division_of_large_numbers() {
    // (2^200 + 12345) / (2^70 + 3): check q * v + r == u and r < v.
    let mut u = [0u32; 8];
    u[6] = 1 << 8;
    u[0] = 12345;
    let v = [3u32, 0, 64];
    let mut q = vec![0u32; 9];
    let mut r = vec![0u32; 3];
    let mut s = vec![0u32; divrem_scratch_len(7, 3)];
    let (ql, rl) = divrem_mag(&u[..7], &v, &mut q, &mut r, &mut s);
    let mut p = vec![0u32; 12];
    let pl = mul_mag(&q[..ql], &v, &mut p);
    let mut sum = vec![0u32; 13];
    let sl = add_mag(&p[..pl], &r[..rl], &mut sum);
    assert_eq!(cmp_mag(&sum[..sl], &u[..7]), Ordering::Equal);
    assert_eq!(cmp_mag(&r[..rl], &v), Ordering::Less);
}

fn decimal(v: &(bool, Vec<u32>)) -> String {
    let mut tmp = vec![0u32; v.1.len()];
    let mut out = vec![0u8; decimal_len(v.1.len())];
    let n = to_decimal(int(v), &mut tmp, &mut out);
    String::from_utf8(out[..n].to_vec()).unwrap()
}

#[test]
fn decimal_round_trip() {
    for &a in &samples() {
        let x = from_i128(a);
        let s = decimal(&x);
        assert_eq!(s, std::format!("{a}"));
        let digits = s.trim_start_matches('-');
        let mut out = vec![0u32; from_decimal_len(digits.len())];
        let n = from_decimal(digits.as_bytes(), &mut out).unwrap();
        assert_eq!(to_i128(a < 0, &out[..n]), a);
    }
    // 100! has 158 digits.
    let mut f = vec![1u32];
    for k in 1..=100u32 {
        let mut out = vec![0u32; f.len() + 1];
        let n = mul_mag(&f, &[k], &mut out);
        f = out[..n].to_vec();
    }
    let s = decimal(&(false, f));
    assert_eq!(s.len(), 158);
    assert!(s.starts_with("93326215443944152681"), "{s}");
    assert!(s.ends_with("000000000000000000000000"), "{s}");
}

#[test]
fn encoding_and_conversions() {
    for &a in &samples() {
        let x = from_i128(a);
        let mut b = vec![0u8; encoded_len(x.1.len())];
        let n = encode(x.0, &x.1, &mut b);
        let mut d = vec![0u32; decoded_len(&b[..n])];
        let (neg, l) = decode(&b[..n], &mut d);
        assert_eq!(to_i128(neg, &d[..l]), a);
        assert_eq!(fits_i64(int(&x)), i64::try_from(a).is_ok(), "{a}");
        assert_eq!(to_i64_wrapping(int(&x)), a as i64, "{a}");
        assert_eq!(to_f64(int(&x)), a as f64, "{a}");
    }
    let mut o = [0u32; 2];
    let (neg, l) = from_i64(i64::MIN, &mut o);
    assert_eq!(to_i128(neg, &o[..l]), i64::MIN as i128);
    for x in [
        0.0,
        0.9,
        -0.9,
        1.5,
        -2.5,
        1e18,
        -9.5e20,
        1e30,
        2f64.powi(100),
    ] {
        let mut out = [0u32; F64_LIMBS];
        let (neg, l) = from_f64(x, &mut out);
        if x.abs() < 1e38 {
            assert_eq!(to_i128(neg, &out[..l]), x.trunc() as i128, "{x}");
        }
    }
    // 2^1023 * 1.5 round-trips through f64.
    let x = 1.5 * 2f64.powi(1023);
    let mut out = [0u32; F64_LIMBS];
    let (neg, l) = from_f64(x, &mut out);
    assert_eq!(
        to_f64(Int {
            neg,
            mag: &out[..l]
        }),
        x
    );
    // Ties round to even: 2^64 + 2^11 (exactly between two doubles).
    let v = [1u32 << 11, 0, 1];
    assert_eq!(
        to_f64(Int {
            neg: false,
            mag: &v
        }),
        2f64.powi(64)
    );
}
