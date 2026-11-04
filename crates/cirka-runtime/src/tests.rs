use crate::object::*;
use crate::TEST_LOCK as LOCK;
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
    let s = unsafe { crate::alloc::STATS.get() };
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
