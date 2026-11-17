// One-dimensional smoothing: 100 sweeps of a three-point stencil (the same
// algorithm and input as main.cirka).
fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    if n < 3 {
        println!("0");
        return;
    }
    let mut a: Vec<f64> = (0..n).map(|i| (i % 100) as f64).collect();
    let mut b = a.clone();
    for _ in 0..100 {
        assert_eq!(a.len(), b.len());
        for i in 1..n - 1 {
            b[i] = 0.25 * a[i - 1] + 0.5 * a[i] + 0.25 * a[i + 1];
        }
        std::mem::swap(&mut a, &mut b);
    }
    let s: f64 = a.iter().sum();
    println!("{}", s as i64);
}
