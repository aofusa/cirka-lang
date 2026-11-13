// String processing: the same work as main.cirka (build a text of `n`
// lines, split it into lines and words, parse numbers, search substrings).
use std::fmt::Write;

fn main() {
    let n: i64 = std::env::args().nth(1).unwrap().parse().unwrap();
    let mut text = String::new();
    for i in 0..n {
        write!(text, "item{} {}  tag{} \n", i, i * 7 % 1000, i % 13).unwrap();
    }
    let (mut w, mut s, mut t, mut b) = (0i64, 0i64, 0i64, 0i64);
    for line in text.lines() {
        let ws: Vec<&str> = line.split_ascii_whitespace().collect();
        w += ws.len() as i64;
        s += ws.get(1).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        if line.contains("tag7") {
            t += 1;
        }
        b += line.trim_ascii().len() as i64;
    }
    println!("({w},{s},{t},{b})");
}
