//! The interpreter's side of the `Integer` externs (FEAT-0114): the same
//! encoded form and algorithms as the runtime (`cirka-runtime/src/bigint.rs`).

use cirka_bigint::{self as big, Int};

fn load(b: &[u8]) -> (bool, Vec<u32>) {
    let mut l = vec![0u32; big::decoded_len(b)];
    let (neg, n) = big::decode(b, &mut l);
    l.truncate(n);
    (neg, l)
}

fn int(v: &(bool, Vec<u32>)) -> Int<'_> {
    Int {
        neg: v.0,
        mag: &v.1,
    }
}

fn store(neg: bool, mag: &[u32]) -> Vec<u8> {
    let mut out = vec![0u8; big::encoded_len(mag.len())];
    let n = big::encode(neg, mag, &mut out);
    out.truncate(n);
    out
}

pub fn from_int(x: i64) -> Vec<u8> {
    let mut l = [0u32; 2];
    let (neg, n) = big::from_i64(x, &mut l);
    store(neg, &l[..n])
}

pub fn fits_int(a: &[u8]) -> i64 {
    i64::from(big::fits_i64(int(&load(a))))
}

pub fn to_int(a: &[u8]) -> i64 {
    big::to_i64_wrapping(int(&load(a)))
}

pub fn to_double(a: &[u8]) -> f64 {
    big::to_f64(int(&load(a)))
}

pub fn from_double(x: f64) -> Vec<u8> {
    let mut l = [0u32; big::F64_LIMBS];
    let (neg, n) = big::from_f64(x, &mut l);
    store(neg, &l[..n])
}

/// 0 add, 1 sub, 2 mul, 3 quot, 4 rem (as `cirka_big_op`).
pub fn op(op: i64, a: &[u8], b: &[u8]) -> Vec<u8> {
    let (x, y) = (load(a), load(b));
    match op {
        0 | 1 => {
            let mut out = vec![0u32; big::add_len(x.1.len(), y.1.len())];
            let (neg, n) = if op == 0 {
                big::add(int(&x), int(&y), &mut out)
            } else {
                big::sub(int(&x), int(&y), &mut out)
            };
            store(neg, &out[..n])
        }
        2 => {
            let mut out = vec![0u32; x.1.len() + y.1.len()];
            let (neg, n) = big::mul(int(&x), int(&y), &mut out);
            store(neg, &out[..n])
        }
        _ => {
            let mut q = vec![0u32; x.1.len() + 1];
            let mut r = vec![0u32; y.1.len().max(1)];
            let mut s = vec![0u32; big::divrem_scratch_len(x.1.len(), y.1.len())];
            let ((qn, ql), (rn, rl)) = big::quot_rem(int(&x), int(&y), &mut q, &mut r, &mut s);
            if op == 3 {
                store(qn, &q[..ql])
            } else {
                store(rn, &r[..rl])
            }
        }
    }
}

pub fn cmp(a: &[u8], b: &[u8]) -> i64 {
    big::cmp(int(&load(a)), int(&load(b))) as i64
}

pub fn show(a: &[u8]) -> Vec<u8> {
    let x = load(a);
    let mut tmp = vec![0u32; x.1.len()];
    let mut out = vec![0u8; big::decimal_len(x.1.len())];
    let n = big::to_decimal(int(&x), &mut tmp, &mut out);
    out.truncate(n);
    out
}

pub fn read(digits: &[u8], neg: bool) -> Vec<u8> {
    let mut l = vec![0u32; big::from_decimal_len(digits.len())];
    let n = big::from_decimal(digits, &mut l).unwrap_or(0);
    store(neg, &l[..n])
}
