// The HTTP server of the benchmark in Rust (FEAT-0243): the same as
// ../server.cirka, on one thread with epoll (level-triggered) and
// non-blocking sockets, with the standard library only.
//
// `server PORT` answers every `GET` request on 127.0.0.1:PORT with
// `Hello, World!`, keeping the connection open; pipelined requests are
// answered with one write. Another method gets 400 and the connection is
// closed. With PORT 0 a free port is chosen; the port is printed first.
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::io::AsRawFd;

const HELLO: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nHello, World!";
const BAD_REQUEST: &[u8] =
    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

// `struct epoll_event` is packed on x86_64.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct EpollEvent {
    events: u32,
    data: u64,
}

extern "C" {
    fn epoll_create1(flags: i32) -> i32;
    fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32;
    fn epoll_wait(epfd: i32, events: *mut EpollEvent, maxevents: i32, timeout: i32) -> i32;
}

const EPOLLIN: u32 = 0x1;
const EPOLLOUT: u32 = 0x4;
const EPOLL_CTL_ADD: i32 = 1;
const EPOLL_CTL_DEL: i32 = 2;
const EPOLL_CTL_MOD: i32 = 3;
const LISTENER: u64 = u64::MAX;

fn ctl(ep: i32, op: i32, fd: i32, events: u32, key: u64) {
    let mut ev = EpollEvent { events, data: key };
    // SAFETY: `ev` is a valid event for the duration of the call.
    let r = unsafe { epoll_ctl(ep, op, fd, &mut ev) };
    assert!(r == 0 || op == EPOLL_CTL_DEL, "epoll_ctl failed");
}

struct Conn {
    stream: TcpStream,
    // Bytes of a request not complete yet.
    buf: Vec<u8>,
    // Answers not sent yet, from `sent`.
    out: Vec<u8>,
    sent: usize,
    // Close once `out` is sent.
    closing: bool,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

// Answers the complete requests in `buf`; false for a bad one.
fn answer(c: &mut Conn) -> bool {
    let mut start = 0;
    while let Some(i) = find(&c.buf[start..], b"\r\n\r\n") {
        if !c.buf[start..].starts_with(b"GET ") {
            return false;
        }
        c.out.extend_from_slice(HELLO);
        start += i + 4;
    }
    c.buf.drain(..start);
    c.buf.len() <= 65536
}

// Writes what it can of `out`: Ok(true) when all is sent.
fn flush(c: &mut Conn) -> std::io::Result<bool> {
    while c.sent < c.out.len() {
        match c.stream.write(&c.out[c.sent..]) {
            Ok(0) => return Err(ErrorKind::WriteZero.into()),
            Ok(n) => c.sent += n,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    c.out.clear();
    c.sent = 0;
    Ok(true)
}

// Handles an event of a connection: false to close it.
fn event(ep: i32, key: u64, c: &mut Conn, events: u32, chunk: &mut [u8]) -> bool {
    if events & EPOLLOUT != 0 || !c.out.is_empty() {
        match flush(c) {
            Ok(true) if c.closing => return false,
            Ok(true) => ctl(ep, EPOLL_CTL_MOD, c.stream.as_raw_fd(), EPOLLIN, key),
            Ok(false) => return true,
            Err(_) => return false,
        }
        return true;
    }
    match c.stream.read(chunk) {
        Ok(0) => return false,
        Ok(n) => c.buf.extend_from_slice(&chunk[..n]),
        Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
            return true;
        }
        Err(_) => return false,
    }
    if !answer(c) {
        c.out.extend_from_slice(BAD_REQUEST);
        c.closing = true;
    }
    if c.out.is_empty() {
        return true;
    }
    match flush(c) {
        Ok(true) => !c.closing,
        Ok(false) => {
            // Wait until the socket can take the rest (like `sendAll`).
            ctl(ep, EPOLL_CTL_MOD, c.stream.as_raw_fd(), EPOLLOUT, key);
            true
        }
        Err(_) => false,
    }
}

fn main() {
    let port: u16 = std::env::args().nth(1).map_or(0, |p| p.parse().unwrap());
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("listen failed");
    listener.set_nonblocking(true).unwrap();
    println!("{}", listener.local_addr().unwrap().port());
    // SAFETY: a plain system call.
    let ep = unsafe { epoll_create1(0) };
    assert!(ep >= 0, "epoll_create1 failed");
    ctl(ep, EPOLL_CTL_ADD, listener.as_raw_fd(), EPOLLIN, LISTENER);
    let mut conns: HashMap<u64, Conn> = HashMap::new();
    let mut next: u64 = 0;
    let mut events = vec![EpollEvent { events: 0, data: 0 }; 256];
    let mut chunk = vec![0u8; 65536];
    loop {
        // SAFETY: `events` has room for `events.len()` entries.
        let n = unsafe { epoll_wait(ep, events.as_mut_ptr(), events.len() as i32, -1) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                continue;
            }
            panic!("epoll_wait failed");
        }
        for ev in &events[..n as usize] {
            let (key, flags) = (ev.data, ev.events);
            if key == LISTENER {
                while let Ok((stream, _)) = listener.accept() {
                    stream.set_nonblocking(true).unwrap();
                    ctl(ep, EPOLL_CTL_ADD, stream.as_raw_fd(), EPOLLIN, next);
                    let c = Conn {
                        stream,
                        buf: Vec::new(),
                        out: Vec::new(),
                        sent: 0,
                        closing: false,
                    };
                    conns.insert(next, c);
                    next += 1;
                }
                continue;
            }
            let keep = match conns.get_mut(&key) {
                Some(c) => event(ep, key, c, flags, &mut chunk),
                None => continue,
            };
            if !keep {
                if let Some(c) = conns.remove(&key) {
                    ctl(ep, EPOLL_CTL_DEL, c.stream.as_raw_fd(), 0, key);
                }
            }
        }
    }
}
