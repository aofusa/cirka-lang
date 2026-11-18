// The session benchmark (the same values as main.cirka): `n` round trips
// between two threads, each the choice to go on, a number and its
// successor as the answer, on channels of the standard library.
use std::sync::mpsc;
use std::thread;

fn main() {
    let n: i64 = std::env::args()
        .nth(1)
        .map(|s| s.parse().unwrap())
        .unwrap_or(100000);
    let (go_tx, go_rx) = mpsc::channel::<bool>();
    let (up_tx, up_rx) = mpsc::channel::<i64>();
    let (down_tx, down_rx) = mpsc::channel::<i64>();
    let server = thread::spawn(move || {
        let mut k: i64 = 0;
        while go_rx.recv().unwrap() {
            let x = up_rx.recv().unwrap();
            down_tx.send(x + 1).unwrap();
            k += 1;
        }
        k
    });
    let mut acc: i64 = 0;
    let mut i = n;
    while i != 0 {
        go_tx.send(true).unwrap();
        up_tx.send(i).unwrap();
        let r = down_rx.recv().unwrap();
        acc = (acc * 31 + r) % 1000000007;
        i -= 1;
    }
    go_tx.send(false).unwrap();
    let k = server.join().unwrap();
    println!("{acc}");
    println!("{k}");
}
