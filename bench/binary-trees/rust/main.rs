// binary-trees (Benchmarks Game), single-threaded Rust baseline:
// `Option<Box<Node>>` (no allocation for empty subtrees, system malloc).
struct Node {
    l: Option<Box<Node>>,
    r: Option<Box<Node>>,
}

fn make(d: u32) -> Box<Node> {
    if d == 0 {
        Box::new(Node { l: None, r: None })
    } else {
        Box::new(Node {
            l: Some(make(d - 1)),
            r: Some(make(d - 1)),
        })
    }
}

fn check(t: &Node) -> i64 {
    1 + t.l.as_deref().map_or(0, check) + t.r.as_deref().map_or(0, check)
}

fn main() {
    let n: u32 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(10);
    let maxd = n.max(6);
    let st = make(maxd + 1);
    println!("stretch tree of depth {}\t check: {}", maxd + 1, check(&st));
    drop(st);
    let ll = make(maxd);
    let mut d = 4;
    while d <= maxd {
        let iters = 1i64 << (maxd - d + 4);
        let mut c = 0;
        for _ in 0..iters {
            let t = make(d);
            c += check(&t);
        }
        println!("{iters}\t trees of depth {d}\t check: {c}");
        d += 2;
    }
    println!("long lived tree of depth {maxd}\t check: {}", check(&ll));
}
