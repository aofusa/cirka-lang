use crate::TEST_LOCK as LOCK;
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
    let _g = LOCK.lock().unwrap();
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
fn shared_child_survives() {
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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

#[test]
fn array_unique_copies_only_shared_arrays() {
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
    let _g = LOCK.lock().unwrap();
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
