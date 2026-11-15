// The load of the HTTP benchmark (FEAT-0243, run.sh), with the standard
// library only.
//
// `load PORT CONNS REQUESTS` opens CONNS connections to 127.0.0.1:PORT
// and sends REQUESTS `GET /` requests on each, one at a time (the next
// once the answer is read), from one thread per connection. Every
// answer must be the `Hello, World!` response. Prints the requests per
// second and the percentiles of the latency (from sending a request to
// reading its answer) in microseconds, as `key=value` pairs.
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Barrier};
use std::time::Instant;

const REQUEST: &[u8] = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
const RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nHello, World!";

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 4 {
        eprintln!("usage: load PORT CONNS REQUESTS");
        std::process::exit(2);
    }
    let port: u16 = a[1].parse().unwrap();
    let conns: usize = a[2].parse().unwrap();
    let requests: usize = a[3].parse().unwrap();
    let start = Arc::new(Barrier::new(conns + 1));
    let threads: Vec<_> = (0..conns)
        .map(|_| {
            let start = start.clone();
            std::thread::spawn(move || {
                let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
                s.set_nodelay(true).unwrap();
                let mut lat = Vec::with_capacity(requests);
                let mut got = vec![0u8; RESPONSE.len()];
                start.wait();
                for _ in 0..requests {
                    let t = Instant::now();
                    s.write_all(REQUEST).expect("send failed");
                    s.read_exact(&mut got).expect("receive failed");
                    lat.push(t.elapsed().as_nanos() as u64);
                    assert!(
                        got == RESPONSE,
                        "unexpected answer: {:?}",
                        String::from_utf8_lossy(&got)
                    );
                }
                lat
            })
        })
        .collect();
    start.wait();
    let t = Instant::now();
    let mut lat: Vec<u64> = Vec::new();
    for h in threads {
        lat.extend(h.join().expect("a connection failed"));
    }
    let secs = t.elapsed().as_secs_f64();
    lat.sort_unstable();
    let pct = |p: f64| lat[((lat.len() as f64 * p) as usize).min(lat.len() - 1)] as f64 / 1000.0;
    println!(
        "requests={} secs={:.3} rps={:.0} p50_us={:.1} p90_us={:.1} p99_us={:.1} max_us={:.1}",
        lat.len(),
        secs,
        lat.len() as f64 / secs,
        pct(0.50),
        pct(0.90),
        pct(0.99),
        pct(1.0)
    );
}
