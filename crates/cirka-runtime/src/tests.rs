use crate::object::*;
const N: usize = 1_000_000;

unsafe fn con(tag: u16, fields: &[usize], nptr: u8) -> Obj {
    unsafe {
        let o = crate::alloc::cirka_alloc(8 + 8 * fields.len()) as Obj;
        (*o).rc = 1;
        (*o).tag = tag;
        (*o).nptr = nptr;
        (*o).kind = CON;
        for (i, f) in fields.iter().enumerate() {
            *word(o, i) = *f;
        }
        o
    }
}

fn stats() -> (u64, u64) {
    let s = unsafe { crate::alloc::stats() };
    (s.allocs, s.frees)
}

#[test]
fn free_long_list_iteratively() {
    let _g = crate::test_lock();
    let (a0, f0) = stats();
    unsafe {
        let mut nil = crate::object::Header {
            rc: 0,
            tag: 0,
            nptr: 0,
            kind: CON,
        };
        let mut list: Obj = &mut nil;
        for i in 0..N {
            let b = con(0, &[i], 0);
            list = con(1, &[b as usize, list as usize], 2);
        }
        // Freeing a million-element list must not overflow the stack.
        cirka_drop(list);
    }
    let (a1, f1) = stats();
    assert_eq!(a1 - a0, 2 * N as u64);
    assert_eq!(f1 - f0, 2 * N as u64);
}

#[test]
fn leaves_and_their_parents_are_freed() {
    // FEAT-1014: an object without pointer fields is released at once, one
    // with pointer fields through the walk (which frees the children that
    // die with it).
    let _g = crate::test_lock();
    let (a0, f0) = stats();
    // SAFETY: objects of this test, each dropped once; `kept` is read
    // while it still holds a reference.
    unsafe {
        let s = crate::cirka_bytes_from(b"leaf".as_ptr(), 4);
        cirka_drop(s);
        let c = con(0, &[1, 2], 0);
        cirka_drop(c);
        let leaf = con(0, &[3], 0);
        let p = con(1, &[leaf as usize, 4], 1);
        cirka_drop(p);
        let kept = con(0, &[5], 0);
        cirka_dup(kept);
        let q = con(1, &[kept as usize], 1);
        cirka_drop(q);
        assert_eq!((*kept).rc, 1);
        assert_eq!(*word(kept, 0), 5);
        cirka_drop(kept);
    }
    let (a1, f1) = stats();
    assert_eq!(a1 - a0, 6);
    assert_eq!(f1 - f0, 6);
}

#[test]
fn shared_child_survives() {
    let _g = crate::test_lock();
    unsafe {
        let leaf = con(0, &[7], 0);
        cirka_dup(leaf);
        let p = con(1, &[leaf as usize, leaf as usize], 2);
        cirka_dup(leaf);
        cirka_drop(p);
        assert_eq!((*leaf).rc, 1);
        assert_eq!(*word(leaf, 0), 7);
        cirka_drop(leaf);
    }
}

#[test]
fn drop_reuse_and_tokens() {
    let _g = crate::test_lock();
    unsafe {
        let leaf = con(0, &[1], 0);
        let p = con(1, &[leaf as usize], 1);
        let tok = cirka_drop_reuse(p);
        assert_eq!(tok, p);
        cirka_free_raw(tok);
        let q = con(1, &[3], 0);
        cirka_dup(q);
        let tok = cirka_drop_reuse(q);
        assert!(tok.is_null());
        assert_eq!((*q).rc, 1);
        cirka_drop(q);
    }
}

#[test]
fn shared_objects_use_atomic_counts() {
    let _g = crate::test_lock();
    unsafe {
        let leaf = con(0, &[1], 0);
        let p = con(1, &[leaf as usize], 1);
        cirka_mark_shared(p);
        assert_eq!((*p).rc, -1);
        assert_eq!((*leaf).rc, -1);
        cirka_dup(p);
        assert_eq!((*p).rc, -2);
        cirka_drop(p);
        cirka_drop(p);
    }
}

#[test]
fn bytes_and_pap() {
    let _g = crate::test_lock();
    unsafe {
        let b = crate::cirka_bytes_from(b"hello".as_ptr(), 5);
        assert_eq!(crate::io::bytes_of(b), b"hello");
        let f = con(0, &[], 0);
        let args = [b];
        let pap = crate::start::cirka_mk_pap(f, 1, args.as_ptr());
        assert_eq!((*pap).nptr, 2);
        cirka_drop(pap);
    }
}

#[test]
fn parse_sizes() {
    assert_eq!(crate::sys::parse_size(b"512M"), Some(512 << 20));
    assert_eq!(crate::sys::parse_size(b"64k"), Some(64 << 10));
    assert_eq!(crate::sys::parse_size(b"123"), Some(123));
    assert_eq!(crate::sys::parse_size(b"x"), None);
}

/// The C header agrees with the Rust definitions.
#[test]
fn header_matches() {
    let h = include_str!("../include/cirka_rt.h");
    for (name, v) in [
        ("CON", CON),
        ("FUN", FUN),
        ("PAP", PAP),
        ("THUNK", THUNK),
        ("BLACKHOLE", BLACKHOLE),
        ("IND", IND),
        ("ARRAY", ARRAY),
        ("BYTES", BYTES),
        ("BIGINT", BIGINT),
        ("FOREIGN", FOREIGN),
        ("MUTVAR", MUTVAR),
        ("MVAR", MVAR),
    ] {
        let line = h
            .lines()
            .find(|l| l.contains(&format!("CIRKA_KIND_{name} ")))
            .unwrap();
        let n: u8 = line.split_whitespace().last().unwrap().parse().unwrap();
        assert_eq!(n, v, "{name}");
    }
    assert_eq!(std::mem::size_of::<Header>(), 8);
}

/// The C header declares every function the libraries' modules of the
/// runtime export.
#[test]
fn header_declares_the_library_functions() {
    let h = include_str!("../include/cirka_rt.h");
    for (file, src) in [
        ("evloop.rs", include_str!("evloop.rs")),
        ("net.rs", include_str!("net.rs")),
        ("os.rs", include_str!("os.rs")),
        ("stdio.rs", include_str!("stdio.rs")),
    ] {
        let mut lines = src.lines();
        let mut n = 0;
        while let Some(l) = lines.next() {
            if l.trim() != "#[unsafe(no_mangle)]" {
                continue;
            }
            let sig = lines.next().unwrap();
            let name = sig.split("fn ").nth(1).unwrap().split('(').next().unwrap();
            assert!(
                h.contains(&format!(" {name}(")) || h.contains(&format!("*{name}(")),
                "{file}: `{name}` is not declared in cirka_rt.h"
            );
            n += 1;
        }
        assert!(n > 5, "{file}: {n} functions");
    }
}

#[test]
fn array_unique_copies_only_shared_arrays() {
    let _g = crate::test_lock();
    unsafe {
        let e = con(0, &[7], 0);
        let a = crate::cirka_array_new(4, e);
        // Unique: returned as is.
        let copies0 = crate::alloc::stats().array_copies;
        let b = crate::cirka_array_unique(a);
        assert_eq!(b, a);
        assert_eq!(crate::alloc::stats().array_copies, copies0);
        // Shared: a copy with the elements dup'ed; the original loses a reference.
        cirka_dup(b);
        let c = crate::cirka_array_unique(b);
        assert_ne!(c, b);
        assert_eq!((*b).rc, 1);
        assert_eq!((*c).rc, 1);
        assert_eq!(*word(c, 0), 4);
        assert_eq!(*word(c, 1), e as usize);
        assert_eq!((*e).rc, 8);
        assert_eq!(crate::alloc::stats().array_copies, copies0 + 1);
        cirka_drop(b);
        assert_eq!((*e).rc, 4);
        cirka_drop(c);
    }
}

// ---- concurrency (FEAT-0045, 05-runtime.md §9) -----------------------------

/// A list deeper than any recursion could mark is marked entirely (the
/// work stack grows on the heap), and freed entirely.
#[test]
fn mark_shared_reaches_deep_structures() {
    let _g = crate::test_lock();
    let (a0, f0) = stats();
    unsafe {
        let mut l = con(0, &[], 0);
        for i in 0..N {
            let h = con(0, &[i], 0);
            l = con(1, &[h as usize, l as usize], 2);
        }
        cirka_mark_shared(l);
        let mut x = l;
        let mut n = 0;
        while (*x).tag == 1 {
            assert!((*x).rc < 0 && (*(*word(x, 0) as Obj)).rc < 0);
            x = *word(x, 1) as Obj;
            n += 1;
        }
        assert_eq!((n, (*x).rc), (N, -1));
        cirka_drop(l);
    }
    let (a1, f1) = stats();
    assert_eq!(a1 - a0, f1 - f0);
}

/// Values pass between threads through a channel and an `MVar`; a value
/// stored into a shared synchronization object is shared.
#[test]
fn mvar_and_channel_between_threads() {
    use crate::sync::*;
    let _g = crate::test_lock();
    set_mt();
    unsafe {
        let m = cirka_mvar_new_empty();
        let c = cirka_chan_new();
        cirka_mark_shared(m);
        cirka_mark_shared(c);
        let (mu, cu) = (m as usize, c as usize);
        let t = std::thread::spawn(move || {
            let (m, c) = (mu as Obj, cu as Obj);
            for i in 0..1000 {
                let v = con(0, &[i], 0);
                cirka_chan_send(c, v);
                cirka_drop(v);
            }
            let v = con(0, &[7], 0);
            cirka_mvar_put(m, v);
            cirka_drop(v);
            crate::alloc::thread_exit();
        });
        let mut sum = 0;
        for _ in 0..1000 {
            let v = cirka_chan_recv(c);
            assert!((*v).rc < 0, "values in a shared channel are shared");
            sum += *word(v, 0);
            cirka_drop(v);
        }
        assert_eq!(sum, 999 * 1000 / 2);
        let v = cirka_mvar_take(m);
        assert_eq!(*word(v, 0), 7);
        // Empty again: `try_take` returns the default.
        let d = con(0, &[0], 0);
        assert_eq!(cirka_mvar_try_take(m, d), d);
        cirka_drop(d);
        assert_eq!(cirka_mvar_try_put(m, v), 1);
        assert_eq!(cirka_mvar_try_put(m, v), 0, "full");
        cirka_drop(v);
        t.join().unwrap();
        cirka_drop(m);
        cirka_drop(c);
    }
}

/// A ping-pong over two channels: each side waits for the other, first
/// watching the box, then sleeping (FEAT-0099). The answers come quickly
/// (caught while watching) and, at the end, after the waiter slept: no
/// change is missed either way.
#[test]
fn ping_pong_between_threads() {
    use crate::sync::*;
    let _g = crate::test_lock();
    set_mt();
    unsafe {
        let up = cirka_chan_new();
        let down = cirka_chan_new();
        cirka_mark_shared(up);
        cirka_mark_shared(down);
        let (uu, du) = (up as usize, down as usize);
        let t = std::thread::spawn(move || {
            let (up, down) = (uu as Obj, du as Obj);
            loop {
                let v = cirka_chan_recv(up);
                let x = *word(v, 0);
                cirka_drop(v);
                if x == usize::MAX {
                    break;
                }
                if x == 9_999 {
                    // Longer than the waiter watches: it sleeps.
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                let r = con(0, &[x + 1], 0);
                cirka_chan_send(down, r);
                cirka_drop(r);
            }
            crate::alloc::thread_exit();
        });
        let mut sum = 0;
        for i in 0..10_000 {
            let v = con(0, &[i], 0);
            cirka_chan_send(up, v);
            cirka_drop(v);
            let r = cirka_chan_recv(down);
            sum += *word(r, 0);
            cirka_drop(r);
        }
        assert_eq!(sum, 10_000 * 10_001 / 2);
        let stop = con(0, &[usize::MAX], 0);
        cirka_chan_send(up, stop);
        cirka_drop(stop);
        t.join().unwrap();
        cirka_drop(up);
        cirka_drop(down);
    }
}

/// A shared thunk forced by several threads at once is evaluated by one of
/// them; the others wait and all get the same value.
#[test]
fn shared_thunk_is_evaluated_once() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn code(_t: Obj) -> Obj {
        RUNS.fetch_add(1, SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(20));
        // SAFETY: a fresh object.
        unsafe { con(0, &[42], 0) }
    }
    let _g = crate::test_lock();
    crate::sync::set_mt();
    unsafe {
        let t = crate::alloc::cirka_alloc(16) as Obj;
        (*t).rc = 1;
        (*t).tag = 0;
        (*t).nptr = 0;
        (*t).kind = THUNK;
        *word(t, 0) = code as *const () as usize;
        cirka_mark_shared(t);
        let tu = t as usize;
        let hs: Vec<_> = (0..8)
            .map(|_| {
                cirka_dup(t);
                std::thread::spawn(move || {
                    let v = crate::start::cirka_force(tu as Obj);
                    let r = v as usize;
                    cirka_drop(v);
                    crate::alloc::thread_exit();
                    r
                })
            })
            .collect();
        let vs: Vec<usize> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(RUNS.load(SeqCst), 1);
        assert!(vs.iter().all(|&v| v == vs[0]));
        assert_eq!(((*t).kind, *word(t, 0)), (IND, vs[0]));
        assert!(
            (*(vs[0] as Obj)).rc < 0,
            "the value of a shared thunk is shared"
        );
        cirka_drop(t);
    }
}

/// A shared reference updated by compare-and-swap from several threads
/// loses no update, and every replaced value is released.
#[test]
fn shared_mutvar_compare_and_swap() {
    use crate::mutvar::*;
    let _g = crate::test_lock();
    crate::sync::set_mt();
    let before = crate::alloc::total_stats();
    unsafe {
        let z = con(0, &[0], 0);
        let r = cirka_mutvar_new(z);
        cirka_drop(z);
        cirka_mark_shared(r);
        let ru = r as usize;
        let hs: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(move || {
                    let r = ru as Obj;
                    let mut done = 0;
                    while done < 1000 {
                        let old = cirka_mutvar_read(r);
                        let new = con(0, &[*word(old, 0) + 1], 0);
                        done += cirka_mutvar_cas(r, old, new);
                        cirka_drop(old);
                        cirka_drop(new);
                    }
                    crate::alloc::thread_exit();
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        let v = cirka_mutvar_read(r);
        assert_eq!(*word(v, 0), 4000);
        cirka_drop(v);
        cirka_drop(r);
    }
    let after = crate::alloc::total_stats();
    assert_eq!(
        after.allocs - before.allocs,
        after.frees - before.frees,
        "every value is released"
    );
}

/// `spawn` runs an IO action on a new thread; `join` returns its (shared)
/// result, as often as asked. (Unit tests have no generated code: the
/// "closure" is a BYTES object holding the function, see `thread.rs`.)
#[test]
fn spawn_and_join() {
    extern "C" fn work(tok: Obj) -> Obj {
        // SAFETY: the token is owned; a fresh result.
        unsafe {
            cirka_drop(tok);
            con(0, &[crate::tls::thread_id() as usize], 0)
        }
    }
    let _g = crate::test_lock();
    unsafe {
        let code = (work as *const () as usize).to_ne_bytes();
        let f = crate::cirka_bytes_from(code.as_ptr(), 8);
        let h = crate::thread::cirka_thread_spawn(f);
        cirka_drop(f);
        let r = crate::thread::cirka_thread_join(h);
        let r2 = crate::thread::cirka_thread_join(h);
        assert_eq!(r, r2);
        assert!(*word(r, 0) >= 2, "spawned threads are numbered from 2");
        assert!((*r).rc < 0, "the result is shared");
        cirka_drop(r);
        cirka_drop(r2);
        cirka_drop(h);
        crate::thread::wait_finishing();
        assert_eq!(crate::thread::running(), 0);
    }
}

/// Every thread allocates from the free lists in its own block
/// (05-runtime.md §9.2).
#[test]
fn every_thread_has_its_own_allocator_state() {
    extern "C" fn work(tok: Obj) -> Obj {
        // SAFETY: the token is owned; a fresh result.
        unsafe {
            cirka_drop(tok);
            con(0, &[crate::tls::heap() as usize], 0)
        }
    }
    let _g = crate::test_lock();
    unsafe {
        let code = (work as *const () as usize).to_ne_bytes();
        let f = crate::cirka_bytes_from(code.as_ptr(), 8);
        let h = crate::thread::cirka_thread_spawn(f);
        cirka_drop(f);
        let r = crate::thread::cirka_thread_join(h);
        let mine = crate::tls::heap();
        assert_eq!(mine, &raw mut (*crate::tls::tls()).heap);
        assert_ne!(*word(r, 0), mine as usize, "the other thread has its own");
        let a0 = crate::alloc::stats().allocs;
        let o = con(0, &[1], 0);
        assert_eq!(crate::alloc::stats().allocs, a0 + 1);
        cirka_drop(o);
        cirka_drop(r);
        cirka_drop(h);
        crate::thread::wait_finishing();
    }
}

/// Objects allocated by one thread and freed by another join the freeing
/// thread's free lists, which go to the pool when it exits.
#[test]
fn objects_freed_by_another_thread() {
    let _g = crate::test_lock();
    let before = crate::alloc::total_stats();
    let objs: Vec<usize> = (0..10_000)
        .map(|i| unsafe { con(0, &[i], 0) } as usize)
        .collect();
    std::thread::spawn(move || {
        for o in objs {
            // SAFETY: the objects were handed over to this thread.
            unsafe { cirka_drop(o as Obj) };
        }
        crate::alloc::thread_exit();
    })
    .join()
    .unwrap();
    let after = crate::alloc::total_stats();
    assert_eq!(after.allocs - before.allocs, 10_000);
    assert_eq!(after.frees - before.frees, 10_000);
}

/// The byte string operations of the support library (FEAT-0095).
#[test]
fn byte_copy_search_and_compare() {
    use crate::bytes::*;
    let _g = crate::test_lock();
    // A BYTES object holding `s`.
    let mk = |s: &[u8]| {
        let o = crate::cirka_bytes_new(s.len() as i64);
        // SAFETY: a fresh BYTES object has room for `s.len()` bytes after
        // its length word.
        unsafe { core::ptr::copy_nonoverlapping(s.as_ptr(), word(o, 1) as *mut u8, s.len()) };
        o
    };
    unsafe {
        let s = mk(b"item7 tag7 \n");
        let t = mk(b"..........");
        // Copies, also within one object (the ranges may overlap).
        assert_eq!(cirka_bytes_copy(t, 2, s, 0, 5), 0);
        assert_eq!(crate::io::bytes_of(t), b"..item7...");
        cirka_bytes_copy(t, 3, t, 2, 5);
        assert_eq!(crate::io::bytes_of(t), b"..iitem7..");
        cirka_bytes_copy(t, 0, s, 0, 0);
        cirka_bytes_copy(t, 0, s, 100, -1);
        assert_eq!(crate::io::bytes_of(t), b"..iitem7..");
        // The first index of a byte in a range; -1 when absent, for an
        // empty range and for a value that is no byte.
        assert_eq!(cirka_bytes_index(s, b'7' as i64, 0, 12), 4);
        assert_eq!(cirka_bytes_index(s, b'7' as i64, 5, 12), 9);
        assert_eq!(cirka_bytes_index(s, b'7' as i64, 5, 9), -1);
        assert_eq!(cirka_bytes_index(s, b'\n' as i64, 3, 3), -1);
        assert_eq!(cirka_bytes_index(s, b'7' as i64 + 256, 0, 12), -1);
        // Ranges compared.
        assert_eq!(cirka_bytes_eq(s, 6, t, 3, 3), 0);
        assert_eq!(cirka_bytes_eq(s, 0, t, 3, 5), 1);
        assert_eq!(cirka_bytes_eq(s, 1, s, 6, 1), 1);
        assert_eq!(cirka_bytes_eq(s, 50, t, 50, 0), 1);
        cirka_drop(s);
        cirka_drop(t);
    }
}

#[test]
fn bytes_resize_in_place_and_by_copy() {
    use crate::bytes::cirka_bytes_resize;
    let _g = crate::test_lock();
    // SAFETY: fresh BYTES objects owned by the test; each resize consumes
    // its argument and returns the object to use instead.
    unsafe {
        let s = crate::cirka_bytes_new(3);
        core::ptr::copy_nonoverlapping(b"abc".as_ptr(), word(s, 1) as *mut u8, 3);
        // A small object grows out of the region; then a large one, beyond
        // what malloc serves by mmap, grows in place.
        let s = cirka_bytes_resize(s, 1000);
        assert_eq!(&crate::io::bytes_of(s)[..4], b"abc\0");
        assert!(crate::io::bytes_of(s)[3..].iter().all(|&b| b == 0));
        *(word(s, 1) as *mut u8).add(999) = b'z';
        let s = cirka_bytes_resize(s, 4 << 20);
        let b = crate::io::bytes_of(s);
        assert_eq!(
            (b.len(), &b[..3], b[999], b[(4 << 20) - 1]),
            (4 << 20, &b"abc"[..], b'z', 0)
        );
        // Shrinking keeps the first bytes (the memory too, unless less
        // than half is still used); a negative size is empty.
        let s = cirka_bytes_resize(s, 3 << 20);
        assert_eq!(crate::io::bytes_of(s).len(), 3 << 20);
        let s = cirka_bytes_resize(s, 2);
        assert_eq!(crate::io::bytes_of(s), b"ab");
        // A shared object is copied; the original keeps its bytes.
        cirka_dup(s);
        let t = cirka_bytes_resize(s, 4);
        assert_ne!(s, t);
        assert_eq!(
            (crate::io::bytes_of(s), crate::io::bytes_of(t)),
            (&b"ab"[..], &b"ab\0\0"[..])
        );
        let t = cirka_bytes_resize(t, -1);
        assert_eq!(crate::io::bytes_of(t), b"");
        cirka_drop(s);
        cirka_drop(t);
    }
}

#[test]
fn empty_strings_are_the_static_one() {
    // FEAT-1002: an empty result is the runtime's static empty string, not
    // a new object; counting and freeing leave it alone.
    let _g = crate::test_lock();
    let (a0, f0) = stats();
    unsafe {
        let e = crate::cirka_bytes_from(core::ptr::null(), 0);
        assert_eq!(e, crate::cirka_bytes_from(b"x".as_ptr(), 0));
        assert_eq!((*e).rc, 0, "immortal");
        assert_eq!((*e).kind, BYTES);
        assert_eq!(*word(e, 0), 0, "no bytes");
        cirka_dup(e);
        cirka_drop(e);
        cirka_drop(e);
        assert_eq!((*e).rc, 0);
        // Made unique for writing, it is copied.
        let u = crate::cirka_bytes_unique(e);
        assert_ne!(u, e);
        assert_eq!((*u).rc, 1);
        cirka_drop(u);
    }
    let (a1, f1) = stats();
    assert_eq!((a1 - a0, f1 - f0), (1, 1), "only the copy");
}

/// The I/O calls made with `syscall` (FEAT-1007) give libc's results: the
/// count, or -1 with `errno` (EAGAIN on an empty non-blocking descriptor,
/// EBADF on a closed one).
#[test]
fn raw_io_calls_give_the_libc_results() {
    use crate::sys;
    use core::ffi::c_void;
    const EAGAIN: i32 = 11;
    const EBADF: i32 = 9;
    const AF_UNIX: i32 = 1;
    const MSG_NOSIGNAL: i32 = 0x4000;
    unsafe extern "C" {
        fn socketpair(domain: i32, ty: i32, proto: i32, sv: *mut i32) -> i32;
    }
    // SAFETY: descriptors made here; buffers of the lengths given.
    unsafe {
        let errno = || *sys::__errno_location();
        let mut buf = [0u8; 8];
        let b = buf.as_mut_ptr() as *mut c_void;
        let mut p = [0i32; 2];
        assert_eq!(
            sys::pipe2(p.as_mut_ptr(), sys::O_NONBLOCK | sys::O_CLOEXEC),
            0
        );
        assert_eq!(sys::read(p[0], b, 8), -1);
        assert_eq!(errno(), EAGAIN);
        assert_eq!(sys::write(p[1], b"abc".as_ptr() as *const c_void, 3), 3);
        assert_eq!(sys::read(p[0], b, 8), 3);
        assert_eq!(&buf[..3], b"abc");
        sys::close(p[0]);
        sys::close(p[1]);
        // -1, not a closed descriptor: other tests open descriptors meanwhile.
        assert_eq!(sys::write(-1, b"x".as_ptr() as *const c_void, 1), -1);
        assert_eq!(errno(), EBADF);

        let mut s = [0i32; 2];
        let ty = sys::SOCK_STREAM | sys::SOCK_NONBLOCK;
        assert_eq!(socketpair(AF_UNIX, ty, 0, s.as_mut_ptr()), 0);
        let none = core::ptr::null_mut();
        assert_eq!(
            sys::recvfrom(s[0], b, 8, 0, none, core::ptr::null_mut()),
            -1
        );
        assert_eq!(errno(), EAGAIN);
        assert_eq!(
            sys::send(s[1], b"hi".as_ptr() as *const c_void, 2, MSG_NOSIGNAL),
            2
        );
        assert_eq!(sys::recvfrom(s[0], b, 8, 0, none, core::ptr::null_mut()), 2);
        assert_eq!(&buf[..2], b"hi");

        let ep = sys::epoll_create1(sys::EPOLL_CLOEXEC);
        assert!(ep >= 0);
        let mut ev = sys::epoll_event {
            events: sys::EPOLLIN,
            data: 7,
        };
        assert_eq!(sys::epoll_ctl(ep, sys::EPOLL_CTL_ADD, s[0], &mut ev), 0);
        let mut out = [sys::epoll_event { events: 0, data: 0 }; 4];
        assert_eq!(sys::epoll_wait(ep, out.as_mut_ptr(), 4, 0), 0);
        assert_eq!(
            sys::send(s[1], b"!".as_ptr() as *const c_void, 1, MSG_NOSIGNAL),
            1
        );
        assert_eq!(sys::epoll_wait(ep, out.as_mut_ptr(), 4, -1), 1);
        let data = out[0].data;
        assert_eq!(data, 7);
        assert_eq!(sys::epoll_ctl(ep, sys::EPOLL_CTL_ADD, -1, &mut ev), -1);
        assert_eq!(errno(), EBADF);
        sys::close(ep);
        sys::close(s[0]);
        sys::close(s[1]);
    }
}
