// In-place quicksort (the same algorithm and input as own.chs).
fn partition(a: &mut [i64], lo: usize, hi: usize) -> usize {
    let p = a[hi];
    let mut i = lo;
    for j in lo..hi {
        if a[j] < p {
            a.swap(i, j);
            i += 1;
        }
    }
    a.swap(i, hi);
    i
}

fn qsort(a: &mut [i64], lo: i64, hi: i64) {
    if lo < hi {
        let m = partition(a, lo as usize, hi as usize) as i64;
        qsort(a, lo, m - 1);
        qsort(a, m + 1, hi);
    }
}

fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let mut a = vec![0i64; n];
    let mut s: i64 = 12345;
    for x in a.iter_mut() {
        s = (s * 1103515245 + 12345) % 2147483648;
        *x = s % 1000000;
    }
    qsort(&mut a, 0, n as i64 - 1);
    let mut acc: i64 = 0;
    let mut ok = true;
    for i in 0..n {
        ok = ok && (i == 0 || a[i - 1] <= a[i]);
        acc = (acc + a[i] * (i as i64 % 7)) % 1000000007;
    }
    let ok = if ok { "True" } else { "False" };
    println!("({acc},{ok})");
}
