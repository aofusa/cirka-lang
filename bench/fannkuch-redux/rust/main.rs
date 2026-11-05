// fannkuch-redux (Benchmarks Game), single-threaded Rust baseline: the
// permutations in the order of the counting algorithm, with the checksum
// alternating signs.
fn fannkuch(n: usize) -> (i64, i64) {
    let mut perm1: Vec<usize> = (0..n).collect();
    let mut perm = vec![0; n];
    let mut count = vec![0usize; n];
    let (mut r, mut perm_count, mut checksum, mut max_flips) = (n, 0i64, 0i64, 0i64);
    loop {
        while r != 1 {
            count[r - 1] = r;
            r -= 1;
        }
        perm.copy_from_slice(&perm1);
        let mut flips = 0;
        let mut k = perm[0];
        while k != 0 {
            perm[..=k].reverse();
            flips += 1;
            k = perm[0];
        }
        max_flips = max_flips.max(flips);
        checksum += if perm_count % 2 == 0 { flips } else { -flips };
        loop {
            if r == n {
                return (checksum, max_flips);
            }
            let p0 = perm1[0];
            perm1.copy_within(1..=r, 0);
            perm1[r] = p0;
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
        perm_count += 1;
    }
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(7);
    let (checksum, flips) = fannkuch(n);
    println!("{checksum}\nPfannkuchen({n}) = {flips}");
}
