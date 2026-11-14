// Binary search over a sorted array (the same algorithm and input as
// main.cirka).
fn search(a: &[i64], key: i64, mut lo: usize, mut hi: usize) -> i64 {
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let x = a[mid];
        if x == key {
            return mid as i64;
        } else if x < key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    -1
}

fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let a: Vec<i64> = (0..n as i64).map(|i| 3 * i + 1).collect();
    let mut s: i64 = 12345;
    let mut acc: i64 = 0;
    for _ in 0..4000 * n {
        s = (s * 1103515245 + 12345) % 2147483648;
        let r = search(&a, s % (3 * n as i64), 0, n);
        acc = (acc + r + 1) % 1000000007;
    }
    println!("{acc}");
}
