// A small CUI tool (FEAT-0215): the same work as ../main.cirka.
use std::fmt::Write as _;

fn main() {
    let n: i64 = std::env::args().nth(1).map_or(1000, |a| a.parse().unwrap());
    let mut text = String::new();
    for i in 0..n {
        let _ = write!(text, "item{} {}  tag{} \n", i, i * 7 % 1000, i % 13);
    }
    let path = format!("/tmp/cirka-bench-cli-grep-rust-{}.txt", std::process::id());
    std::fs::write(&path, &text).unwrap();
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let (mut l, mut w, mut m) = (0, 0, 0);
    let mut firsts = Vec::new();
    for line in contents.lines() {
        l += 1;
        w += line.split_ascii_whitespace().count();
        if line.contains("tag7") {
            if m < 3 {
                firsts.push(line);
            }
            m += 1;
        }
    }
    for f in firsts {
        println!("{f}");
    }
    println!("({},{},{},{})", l, w, contents.len(), m);
}
