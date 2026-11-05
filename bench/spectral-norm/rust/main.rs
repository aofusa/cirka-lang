// spectral-norm (Benchmarks Game), single-threaded Rust baseline (no SIMD).
fn a(i: usize, j: usize) -> f64 {
    1.0 / (((i + j) * (i + j + 1) / 2 + i + 1) as f64)
}

fn mul_av(u: &[f64], v: &mut [f64]) {
    for (i, vi) in v.iter_mut().enumerate() {
        *vi = u.iter().enumerate().map(|(j, uj)| a(i, j) * uj).sum();
    }
}

fn mul_atv(u: &[f64], v: &mut [f64]) {
    for (i, vi) in v.iter_mut().enumerate() {
        *vi = u.iter().enumerate().map(|(j, uj)| a(j, i) * uj).sum();
    }
}

fn mul_atav(u: &[f64], v: &mut [f64], t: &mut [f64]) {
    mul_av(u, t);
    mul_atv(t, v);
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(100);
    let mut u = vec![1.0; n];
    let mut v = vec![0.0; n];
    let mut t = vec![0.0; n];
    for _ in 0..10 {
        mul_atav(&u, &mut v, &mut t);
        mul_atav(&v, &mut u, &mut t);
    }
    let vbv: f64 = u.iter().zip(&v).map(|(a, b)| a * b).sum();
    let vv: f64 = v.iter().map(|b| b * b).sum();
    println!("{:.9}", (vbv / vv).sqrt());
}
