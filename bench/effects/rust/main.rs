// The effects benchmark (the same values as main.cirka): `n` steps of a
// hash, the hash so far printed every `n / 4` steps.
fn mix(a: i64, b: i64) -> i64 {
    (a * 31 + b) % 1000000007
}

fn main() {
    let n: i64 = std::env::args()
        .nth(1)
        .map(|s| s.parse().unwrap())
        .unwrap_or(1000000);
    let every = if n < 4 { 1 } else { n / 4 };
    let mut acc = 0;
    let mut i = n;
    while i != 0 {
        acc = mix(acc, i);
        if i % every == 0 {
            println!("{acc}");
        }
        i -= 1;
    }
    println!("{acc}");
}
