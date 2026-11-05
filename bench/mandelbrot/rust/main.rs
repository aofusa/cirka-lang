// mandelbrot (Benchmarks Game algorithm), single-threaded Rust baseline.
// Instead of the PBM bitmap it prints the number of points of the n x n
// grid in the set (50 iterations, escape radius 2), which every
// implementation can print as text.
fn inside(cr: f64, ci: f64) -> bool {
    let (mut zr, mut zi) = (0.0f64, 0.0f64);
    for _ in 0..50 {
        let (tr, ti) = (zr * zr, zi * zi);
        if tr + ti > 4.0 {
            return false;
        }
        zi = 2.0 * zr * zi + ci;
        zr = tr - ti + cr;
    }
    true
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(200);
    let mut count = 0u64;
    for y in 0..n {
        let ci = 2.0 * y as f64 / n as f64 - 1.0;
        for x in 0..n {
            let cr = 2.0 * x as f64 / n as f64 - 1.5;
            if inside(cr, ci) {
                count += 1;
            }
        }
    }
    println!("{count}");
}
