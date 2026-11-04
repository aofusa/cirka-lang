//! Debug runtime checks (05-runtime.md §8), enabled by the `debug-rt`
//! feature:
//!
//! - every object has a 48-byte prefix with a magic value, allocation number
//!   and links in a list of live objects,
//! - freed objects are poisoned and quarantined (not reused immediately),
//! - dup/drop check that the object is alive,
//! - at exit, `CIRKA_LEAK_CHECK=1` reports objects that are still alive and
//!   not reachable from a CAF.

#![cfg_attr(feature = "debug-rt", allow(dead_code, unused_imports))]

use crate::object::Obj;

#[cfg(feature = "debug-rt")]
pub use imp::*;

/// Checks that `o` is a valid live object (no-op in the release runtime).
#[cfg(not(feature = "debug-rt"))]
#[inline(always)]
pub unsafe fn check_live(_o: Obj, _what: &str) {}

/// Number of live objects not reachable from CAFs (release runtime: unknown).
#[cfg(not(feature = "debug-rt"))]
pub unsafe fn leak_report(_roots: &[Obj], _print: bool) -> u64 {
    0
}

#[cfg(feature = "debug-rt")]
mod imp {
    use crate::Global;
    use crate::io;
    use crate::object::{Obj, ptr_fields, word};
    use crate::sys;
    use core::ffi::c_void;

    const MAGIC: u64 = 0xC1_4C_A0_B1_EC_7A_11_7E;
    const DEAD: u64 = 0xDEAD_0B1E_C7DE_AD00;
    const PREFIX: usize = 48;
    const QUARANTINE: usize = 4096;

    #[repr(C)]
    struct Prefix {
        magic: u64,
        id: u64,
        size: u64,
        prev: *mut Prefix,
        next: *mut Prefix,
        mark: u64,
    }

    struct State {
        live: *mut Prefix,
        next_id: u64,
        quarantine: [*mut Prefix; QUARANTINE],
        qpos: usize,
    }

    static STATE: Global<State> = Global::new(State {
        live: core::ptr::null_mut(),
        next_id: 1,
        quarantine: [core::ptr::null_mut(); QUARANTINE],
        qpos: 0,
    });

    fn prefix(o: Obj) -> *mut Prefix {
        (o as usize - PREFIX) as *mut Prefix
    }

    pub unsafe fn debug_alloc(size: usize) -> *mut u8 {
        // SAFETY: single-threaded debug allocator.
        unsafe {
            let p = sys::malloc(PREFIX + size.max(8)) as *mut Prefix;
            if p.is_null() {
                io::fatal(b"out of memory", 2);
            }
            let st = STATE.get();
            (*p).magic = MAGIC;
            (*p).id = st.next_id;
            st.next_id += 1;
            (*p).size = size as u64;
            (*p).prev = core::ptr::null_mut();
            (*p).next = st.live;
            (*p).mark = 0;
            if !st.live.is_null() {
                (*st.live).prev = p;
            }
            st.live = p;
            (p as *mut u8).add(PREFIX)
        }
    }

    pub unsafe fn debug_free(o: *mut u8) {
        // SAFETY: o was returned by debug_alloc and is dead.
        unsafe {
            let p = prefix(o as Obj);
            if (*p).magic != MAGIC {
                io::fatal(b"cirka debug runtime: double free or corrupted object", 3);
            }
            let st = STATE.get();
            if !(*p).prev.is_null() {
                (*(*p).prev).next = (*p).next;
            } else {
                st.live = (*p).next;
            }
            if !(*p).next.is_null() {
                (*(*p).next).prev = (*p).prev;
            }
            (*p).magic = DEAD;
            sys::memset(o as *mut c_void, 0xDE, (*p).size as usize);
            let old = st.quarantine[st.qpos];
            st.quarantine[st.qpos] = p;
            st.qpos = (st.qpos + 1) % QUARANTINE;
            if !old.is_null() {
                sys::free(old as *mut c_void);
            }
        }
    }

    pub unsafe fn check_live(o: Obj, what: &str) {
        // SAFETY: reading the header and prefix of a (supposedly) live object.
        unsafe {
            if o.is_null() {
                fail(what, b"null object");
            }
            if (*o).rc == 0 {
                return; // static / immortal
            }
            let p = prefix(o);
            if (*p).magic == DEAD {
                fail(
                    what,
                    b"object was already freed (use after free / double drop)",
                );
            }
            if (*p).magic != MAGIC {
                fail(what, b"not a heap object or corrupted header");
            }
        }
    }

    fn fail(what: &str, msg: &[u8]) -> ! {
        io::ewrite(b"cirka debug runtime: ");
        io::ewrite(what.as_bytes());
        io::ewrite(b": ");
        io::fatal(msg, 3)
    }

    unsafe fn mark(o: Obj) {
        // SAFETY: traversal of live objects reachable from CAFs.
        unsafe {
            let mut stack = [core::ptr::null_mut::<crate::object::Header>(); 256];
            let mut sp = 1;
            stack[0] = o;
            while sp > 0 {
                sp -= 1;
                let x = stack[sp];
                if x.is_null() {
                    continue;
                }
                if (*x).rc != 0 {
                    let p = prefix(x);
                    if (*p).magic != MAGIC || (*p).mark != 0 {
                        continue;
                    }
                    (*p).mark = 1;
                }
                let (start, count) = ptr_fields(x, (*x).kind, (*x).nptr);
                for i in start..start + count {
                    let c = *word(x, i) as Obj;
                    if sp < stack.len() {
                        stack[sp] = c;
                        sp += 1;
                    } else {
                        mark(c);
                    }
                }
            }
        }
    }

    /// Counts (and optionally prints) live objects not reachable from `roots`.
    pub unsafe fn leak_report(roots: &[Obj], print: bool) -> u64 {
        // SAFETY: single-threaded at exit.
        unsafe {
            for r in roots {
                mark(*r);
            }
            let mut n = 0;
            let mut p = STATE.get().live;
            while !p.is_null() {
                if (*p).mark == 0 {
                    n += 1;
                    if print && n <= 20 {
                        let o = (p as *mut u8).add(PREFIX) as Obj;
                        io::ewrite(b"cirka: leaked object #");
                        io::ewrite_u64((*p).id);
                        io::ewrite(b" kind=");
                        io::ewrite_u64((*o).kind as u64);
                        io::ewrite(b" tag=");
                        io::ewrite_u64((*o).tag as u64);
                        io::ewrite(b" size=");
                        io::ewrite_u64((*p).size);
                        io::ewrite(b"\n");
                    }
                }
                p = (*p).next;
            }
            n
        }
    }
}
