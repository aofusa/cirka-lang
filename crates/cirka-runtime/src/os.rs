//! The operating system layer of the libraries (FEAT-0242, 05-runtime.md
//! §11, spec-v0.0.5.md §3): child processes, signals, memory-mapped files,
//! directories and file metadata, the working directory and host name.
//!
//! Failures return `-errno`, or an empty value with `errno` set (the
//! convention of `stdio.rs`). Results beyond one integer stay in the
//! thread's state for `cirka_os_result` to read: the pipes of a spawn, the
//! status of a wait, the fields of a stat, the length of the output of
//! `cirka_os_communicate`.

use crate::evloop::{budget_left, foreign, spend};
use crate::object::{FOREIGN, Obj, word};
use crate::{io, sys, tls};
use core::ffi::{c_char, c_int, c_void};

const EINTR: i32 = 4;
const EAGAIN: i32 = 11;
const ENOMEM: i32 = 12;
const EINVAL: i32 = 22;
const EPIPE: i32 = 32;
const ERANGE: i32 = 34;
const SIGBUS: c_int = 7;
const SIGSEGV: c_int = 11;
const SIGPIPE: c_int = 13;

fn errno() -> i32 {
    // SAFETY: the thread's errno location.
    unsafe { *sys::__errno_location() }
}

fn set_errno(e: i32) {
    // SAFETY: the thread's errno location.
    unsafe { *sys::__errno_location() = e }
}

fn neg_errno() -> i64 {
    -i64::from(errno())
}

/// The extra result `i` of the last call of this module on this thread
/// (0 if there is none): after `cirka_os_spawn` the parent's ends of the
/// pipes of the standard input, output and error (0, 1, 2; -1 for a
/// stream that is not a pipe); after `cirka_os_wait` the exit code (0; -1
/// if a signal ended the child) and the signal (1; 0 if none); after
/// `cirka_fs_stat` the size (0), kind (1: 1 file, 2 directory, 3 symbolic
/// link, 0 other), permissions (2) and modification time in seconds (3);
/// after `cirka_os_communicate` the length of the output (0).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_result(i: i64) -> i64 {
    match usize::try_from(i) {
        // SAFETY: this thread's block.
        Ok(i) if i < 6 => unsafe { (*tls::tls()).os[i] },
        _ => 0,
    }
}

fn set_result(i: usize, v: i64) {
    // SAFETY: this thread's block.
    unsafe { (*tls::tls()).os[i] = v }
}

/// A NUL-terminated copy of a string in a malloc'ed buffer, freed when
/// dropped.
struct CBuf(*mut c_char);

impl CBuf {
    /// `s` as a C string; `Err(-EINVAL)` if it contains a NUL.
    ///
    /// # Safety
    /// `s` must be a live BYTES object.
    unsafe fn of(s: Obj) -> Result<CBuf, i64> {
        // SAFETY: guaranteed by the caller.
        let b = unsafe { io::bytes_of(s) };
        if b.contains(&0) {
            return Err(-i64::from(EINVAL));
        }
        Ok(CBuf::from_bytes(b))
    }

    /// `b` (without NULs) followed by a NUL.
    fn from_bytes(b: &[u8]) -> CBuf {
        // SAFETY: a fresh buffer of len + 1 bytes.
        unsafe {
            let p = sys::malloc(b.len() + 1) as *mut u8;
            if p.is_null() {
                io::fatal(b"out of memory", 251);
            }
            core::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
            *p.add(b.len()) = 0;
            CBuf(p as *mut c_char)
        }
    }
}

impl Drop for CBuf {
    fn drop(&mut self) {
        // SAFETY: allocated by `from_bytes`, freed once.
        unsafe { sys::free(self.0 as *mut c_void) }
    }
}

/// A growable malloc'ed byte buffer.
struct Buf {
    p: *mut u8,
    len: usize,
    cap: usize,
}

impl Buf {
    fn new() -> Buf {
        Buf {
            p: core::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    fn push(&mut self, b: &[u8]) {
        if b.is_empty() {
            return;
        }
        if self.len + b.len() > self.cap {
            let cap = (self.len + b.len()).max(self.cap * 2).max(256);
            // SAFETY: grows the buffer (realloc of null allocates).
            let p = unsafe { sys::realloc(self.p as *mut c_void, cap) } as *mut u8;
            if p.is_null() {
                io::fatal(b"out of memory", 251);
            }
            self.p = p;
            self.cap = cap;
        }
        // SAFETY: room for `b` was made above.
        unsafe { core::ptr::copy_nonoverlapping(b.as_ptr(), self.p.add(self.len), b.len()) };
        self.len += b.len();
    }

    fn bytes(&self) -> &[u8] {
        if self.p.is_null() {
            &[]
        } else {
            // SAFETY: `len` initialized bytes.
            unsafe { core::slice::from_raw_parts(self.p, self.len) }
        }
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        // SAFETY: allocated by `push` (or null).
        unsafe { sys::free(self.p as *mut c_void) }
    }
}

/// A runtime string of `b`.
fn string(b: &[u8]) -> Obj {
    // SAFETY: copies `b`.
    unsafe { crate::cirka_bytes_from(b.as_ptr(), b.len()) }
}

/// Closes `fd` if it is one.
fn close(fd: c_int) {
    if fd >= 0 {
        // SAFETY: the caller's own descriptor.
        unsafe { sys::close(fd) };
    }
}

// ---- child processes ----

/// How a standard stream of a child is connected: two bits per stream
/// (input, output, error) of the `flags` of `cirka_os_spawn`.
const INHERIT: i64 = 0;
const PIPE: i64 = 1;
const NULL: i64 = 2;
/// Flag: the child's error goes where its output goes.
const ERR_TO_OUT: i64 = 64;

/// Starts the program `argv` (the program and its arguments, each ended
/// by a NUL; the program is looked up in `PATH`) as a child process with
/// this process's environment, an empty signal mask and the default
/// action for `SIGPIPE` (as from a shell, even if this process ignores
/// it). `flags` connects
/// its standard streams (`INHERIT`, `PIPE` or `NULL` for each, and
/// `ERR_TO_OUT`). Returns the process id, or `-errno` (`-ENOENT` if there
/// is no such program); the parent's ends of the pipes are the results 0
/// to 2 (close-on-exec, so other children do not inherit them).
///
/// # Safety
/// `argv` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_spawn(argv: Obj, flags: i64) -> i64 {
    // SAFETY: `argv` is live; the C structures are initialized before use
    // and destroyed after; the pointers passed to posix_spawnp point into
    // `buf`, which outlives the call.
    unsafe {
        let b = io::bytes_of(argv);
        let body = b.strip_suffix(&[0]).unwrap_or(b);
        if body.is_empty() {
            return -i64::from(EINVAL);
        }
        let buf = CBuf::from_bytes(body);
        let n = body.iter().filter(|&&c| c == 0).count() + 1;
        let ptrs = sys::malloc((n + 1) * size_of::<*const c_char>()) as *mut *const c_char;
        if ptrs.is_null() {
            io::fatal(b"out of memory", 251);
        }
        let mut k = 0;
        *ptrs = buf.0;
        for (i, &c) in body.iter().enumerate() {
            if c == 0 {
                k += 1;
                *ptrs.add(k) = buf.0.add(i + 1);
            }
        }
        *ptrs.add(n) = core::ptr::null();
        let mut fa: sys::posix_spawn_file_actions_t = core::mem::zeroed();
        let mut attr: sys::posix_spawnattr_t = core::mem::zeroed();
        sys::posix_spawn_file_actions_init(&mut fa);
        sys::posix_spawnattr_init(&mut attr);
        // The child starts with no signal blocked, whatever this thread
        // blocks to receive signals (`cirka_os_signalfd`).
        let empty = sys::sigset_t { bits: [0; 16] };
        sys::posix_spawnattr_setsigmask(&mut attr, &empty);
        let mut pipe = sys::sigset_t { bits: [0; 16] };
        pipe.bits[0] = 1 << (SIGPIPE - 1);
        sys::posix_spawnattr_setsigdefault(&mut attr, &pipe);
        sys::posix_spawnattr_setflags(
            &mut attr,
            sys::POSIX_SPAWN_SETSIGMASK | sys::POSIX_SPAWN_SETSIGDEF,
        );
        let mut parent = [-1 as c_int; 3];
        let mut child = [-1 as c_int; 3];
        let null = c"/dev/null";
        let mut failed = 0;
        for i in 0..3 {
            match (flags >> (2 * i)) & 3 {
                PIPE => {
                    let mut fds = [-1 as c_int; 2];
                    if sys::pipe2(fds.as_mut_ptr(), sys::O_CLOEXEC) < 0 {
                        failed = errno();
                        break;
                    }
                    // The child reads its input from the read end and
                    // writes its output to the write end.
                    let (c, p) = if i == 0 {
                        (fds[0], fds[1])
                    } else {
                        (fds[1], fds[0])
                    };
                    child[i] = c;
                    parent[i] = p;
                    sys::posix_spawn_file_actions_adddup2(&mut fa, c, i as c_int);
                }
                NULL => {
                    sys::posix_spawn_file_actions_addopen(
                        &mut fa,
                        i as c_int,
                        null.as_ptr(),
                        sys::O_RDWR,
                        0,
                    );
                }
                INHERIT => {}
                _ => failed = EINVAL,
            }
        }
        if flags & ERR_TO_OUT != 0 {
            sys::posix_spawn_file_actions_adddup2(&mut fa, 1, 2);
        }
        let mut pid: c_int = 0;
        if failed == 0 {
            failed = sys::posix_spawnp(&mut pid, buf.0, &fa, &attr, ptrs, sys::environ);
        }
        sys::posix_spawn_file_actions_destroy(&mut fa);
        sys::posix_spawnattr_destroy(&mut attr);
        sys::free(ptrs as *mut c_void);
        for c in child {
            close(c);
        }
        if failed != 0 {
            for p in parent {
                close(p);
            }
            return -i64::from(failed);
        }
        for (i, p) in parent.iter().enumerate() {
            set_result(i, i64::from(*p));
        }
        i64::from(pid)
    }
}

/// Waits for the child `pid` to end (with `nohang`, only looks): 0 when it
/// ended (its exit code and signal are the results 0 and 1), 1 if it is
/// still running (`nohang`), or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_wait(pid: i64, nohang: i64) -> i64 {
    let opts = if nohang != 0 { sys::WNOHANG } else { 0 };
    loop {
        let mut st: c_int = 0;
        // SAFETY: plain system call.
        let r = unsafe { sys::waitpid(pid as c_int, &mut st, opts) };
        if r < 0 {
            if errno() == EINTR {
                continue;
            }
            return neg_errno();
        }
        if r == 0 {
            return 1;
        }
        let sig = st & 0x7f;
        if sig == 0 {
            set_result(0, i64::from((st >> 8) & 0xff));
            set_result(1, 0);
        } else {
            set_result(0, -1);
            set_result(1, i64::from(sig));
        }
        return 0;
    }
}

/// Sends the signal `sig` to the process `pid`: 0 or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_kill(pid: i64, sig: i64) -> i64 {
    // SAFETY: plain system call.
    if unsafe { sys::kill(pid as c_int, sig as c_int) } < 0 {
        neg_errno()
    } else {
        0
    }
}

/// Sets `fd` non-blocking.
fn nonblocking(fd: c_int) {
    // SAFETY: plain system calls on the caller's descriptor.
    unsafe {
        let fl = sys::fcntl(fd, sys::F_GETFL);
        if fl >= 0 {
            sys::fcntl(fd, sys::F_SETFL, fl | sys::O_NONBLOCK);
        }
    }
}

/// Writes `input` to `inp` (then closes it) while reading `out` and `err`
/// until their ends, without waiting on one pipe while the child waits on
/// another (-1 for a stream that is not used). Takes the descriptors and
/// closes them. Returns what `out` gave followed by what `err` gave, the
/// length of the first part as result 0; a failure of `poll` is `errno`
/// with what was read so far. A child that stops reading its input does
/// not end this process with `SIGPIPE`: the rest of the input is dropped.
///
/// # Safety
/// `input` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_communicate(inp: i64, out: i64, err: i64, input: Obj) -> Obj {
    // SAFETY: `input` is live; the descriptors are the caller's; the
    // signal mask is restored before returning.
    unsafe {
        let data = io::bytes_of(input);
        let mut fds = [inp as c_int, out as c_int, err as c_int];
        for &fd in &fds {
            if fd >= 0 {
                nonblocking(fd);
            }
        }
        let mut pipe = sys::sigset_t { bits: [0; 16] };
        pipe.bits[0] = 1 << (SIGPIPE - 1);
        let mut old = sys::sigset_t { bits: [0; 16] };
        sys::pthread_sigmask(sys::SIG_BLOCK, &pipe, &mut old);
        let mut sent = 0usize;
        if fds[0] >= 0 && data.is_empty() {
            close(fds[0]);
            fds[0] = -1;
        }
        let mut bufs = [Buf::new(), Buf::new()];
        let mut chunk = [0u8; 16384];
        let mut e = 0;
        while fds.iter().any(|&fd| fd >= 0) {
            let mut pfd: [sys::pollfd; 3] = core::mem::zeroed();
            for i in 0..3 {
                pfd[i].fd = fds[i];
                pfd[i].events = if i == 0 { sys::POLLOUT } else { sys::POLLIN };
            }
            if sys::poll(pfd.as_mut_ptr(), 3, -1) < 0 {
                if errno() == EINTR {
                    continue;
                }
                e = errno();
                break;
            }
            if fds[0] >= 0 && pfd[0].revents != 0 {
                let n = sys::write(
                    fds[0],
                    data.as_ptr().add(sent) as *const c_void,
                    data.len() - sent,
                );
                if n > 0 {
                    sent += n as usize;
                }
                if sent == data.len() || (n < 0 && errno() != EAGAIN && errno() != EINTR) {
                    close(fds[0]);
                    fds[0] = -1;
                }
            }
            for i in 1..3 {
                if fds[i] >= 0 && pfd[i].revents != 0 {
                    let n = sys::read(fds[i], chunk.as_mut_ptr() as *mut c_void, chunk.len());
                    if n > 0 {
                        bufs[i - 1].push(&chunk[..n as usize]);
                    } else if n == 0 || (errno() != EAGAIN && errno() != EINTR) {
                        close(fds[i]);
                        fds[i] = -1;
                    }
                }
            }
        }
        for fd in fds {
            close(fd);
        }
        // Takes the SIGPIPE of a write to a child that stopped reading,
        // unless one was already pending before.
        let zero = sys::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if old.bits[0] & (1 << (SIGPIPE - 1)) == 0 {
            let mut info: sys::siginfo_t = core::mem::zeroed();
            while sys::sigtimedwait(&pipe, &mut info, &zero) == SIGPIPE {}
        }
        sys::pthread_sigmask(sys::SIG_SETMASK, &old, core::ptr::null_mut());
        let mut all = Buf::new();
        all.push(bufs[0].bytes());
        all.push(bufs[1].bytes());
        set_result(0, bufs[0].len as i64);
        set_errno(e);
        string(all.bytes())
    }
}

// ---- signals ----

/// The signals of `mask` (bit `s - 1` for the signal `s`, 1 to 64),
/// without `SIGSEGV` and `SIGBUS`, which the runtime uses to report stack
/// overflows.
fn sigset(mask: i64) -> sys::sigset_t {
    let mut s = sys::sigset_t { bits: [0; 16] };
    s.bits[0] = (mask as u64) & !(1 << (SIGSEGV - 1)) & !(1 << (SIGBUS - 1));
    s
}

/// Blocks in the calling thread every signal but those a fault raises
/// (`SIGILL`, `SIGTRAP`, `SIGBUS`, `SIGFPE`, `SIGSEGV`, `SIGSYS`) and
/// returns the previous mask: threads of the runtime started meanwhile
/// begin with these signals blocked (05-runtime.md §11.3).
pub(crate) fn block_async_signals() -> sys::sigset_t {
    let faults = [4, 5, 7, 8, 11, 31];
    let mut s = sys::sigset_t { bits: [!0; 16] };
    for f in faults {
        s.bits[0] &= !(1u64 << (f - 1));
    }
    let mut old = sys::sigset_t { bits: [0; 16] };
    // SAFETY: valid sets.
    unsafe { sys::pthread_sigmask(sys::SIG_BLOCK, &s, &mut old) };
    old
}

/// Sets the signal mask of the calling thread.
pub(crate) fn set_signal_mask(mask: &sys::sigset_t) {
    // SAFETY: a valid set.
    unsafe { sys::pthread_sigmask(sys::SIG_SETMASK, mask, core::ptr::null_mut()) };
}

/// Sets what the signal `sig` does: `action` 0 its default, 1 nothing
/// (ignored). 0 or `-errno` (`-EINVAL` for `SIGSEGV` and `SIGBUS`, which
/// the runtime handles).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_signal_action(sig: i64, action: i64) -> i64 {
    if sig == i64::from(SIGSEGV) || sig == i64::from(SIGBUS) || !(0..=1).contains(&action) {
        return -i64::from(EINVAL);
    }
    let act = sys::sigaction_t {
        sa_sigaction: action as usize,
        sa_mask: [0; 16],
        sa_flags: 0,
        sa_restorer: 0,
    };
    // SAFETY: a valid action (SIG_DFL or SIG_IGN).
    if unsafe { sys::sigaction(sig as c_int, &act, core::ptr::null_mut()) } < 0 {
        neg_errno()
    } else {
        0
    }
}

/// Sends the signal `sig` to the calling thread: 0 or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_raise(sig: i64) -> i64 {
    // SAFETY: plain call.
    let r = unsafe { sys::raise(sig as c_int) };
    if r != 0 { neg_errno() } else { 0 }
}

/// Blocks (`how` 0) or unblocks (1) the signals of `mask` in the calling
/// thread (threads started later inherit its mask): 0 or `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_signal_mask(how: i64, mask: i64) -> i64 {
    let how = match how {
        0 => sys::SIG_BLOCK,
        1 => sys::SIG_UNBLOCK,
        _ => return -i64::from(EINVAL),
    };
    let s = sigset(mask);
    // SAFETY: valid sets.
    let r = unsafe { sys::pthread_sigmask(how, &s, core::ptr::null_mut()) };
    if r != 0 { -i64::from(r) } else { 0 }
}

/// Waits for one of the signals of `mask`, which must be blocked, for at
/// most `ms` milliseconds if `ms` >= 0: the signal, or `-errno` (`-EAGAIN`
/// when the time passed).
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_signal_wait(mask: i64, ms: i64) -> i64 {
    let s = sigset(mask);
    let ts = sys::timespec {
        tv_sec: ms.max(0) / 1000,
        tv_nsec: (ms.max(0) % 1000) * 1_000_000,
    };
    loop {
        // SAFETY: valid set, timeout and information buffer.
        let r = unsafe {
            let mut info: sys::siginfo_t = core::mem::zeroed();
            sys::sigtimedwait(&s, &mut info, if ms < 0 { core::ptr::null() } else { &ts })
        };
        if r > 0 {
            return i64::from(r);
        }
        if errno() != EINTR {
            return neg_errno();
        }
    }
}

/// Blocks the signals of `mask` in the calling thread and returns a
/// non-blocking, close-on-exec descriptor that reads them (`signalfd`), or
/// `-errno`.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_signalfd(mask: i64) -> i64 {
    let s = sigset(mask);
    // SAFETY: valid sets.
    unsafe {
        let r = sys::pthread_sigmask(sys::SIG_BLOCK, &s, core::ptr::null_mut());
        if r != 0 {
            return -i64::from(r);
        }
        let fd = sys::signalfd(-1, &s, sys::SFD_NONBLOCK | sys::SFD_CLOEXEC);
        if fd < 0 { neg_errno() } else { i64::from(fd) }
    }
}

/// Reads one signal from the signal descriptor of the descriptor object
/// `h`: the signal, or `-errno` (`-EAGAIN` when none is pending or the
/// budget of the running callback is spent).
///
/// # Safety
/// `h` must be a live descriptor object (`cirka_fd_handle`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_signalfd_read(h: Obj) -> i64 {
    // SAFETY: guaranteed by the caller; reads one record into a buffer of
    // its size.
    unsafe {
        let fd = crate::net::cirka_handle_fd(h);
        if fd < 0 {
            return -9;
        }
        if !budget_left() {
            return -i64::from(EAGAIN);
        }
        let mut info: sys::signalfd_siginfo = core::mem::zeroed();
        let n = sys::read(
            fd as c_int,
            &mut info as *mut _ as *mut c_void,
            size_of::<sys::signalfd_siginfo>(),
        );
        if n < 0 {
            return neg_errno();
        }
        spend();
        i64::from(info.ssi_signo)
    }
}

// ---- memory-mapped files ----

/// The state of a mapping object.
struct Mapping {
    ptr: *mut u8,
    len: usize,
}

/// The finalizer of a mapping object: unmaps the file.
extern "C" fn mapping_finalize(m: usize) {
    let m = m as *mut Mapping;
    // SAFETY: the object owned the state and the mapping.
    unsafe {
        if (*m).len > 0 {
            sys::munmap((*m).ptr as *mut c_void, (*m).len);
        }
        sys::free(m as *mut c_void);
    }
}

/// Maps the file at `path` read-only (private): a mapping object (kind
/// `FOREIGN`) that unmaps it when freed, with `errno` 0; on failure an
/// empty mapping with `errno` set. An empty file maps to an empty mapping.
/// A file that another process truncates while it is mapped ends this
/// process with `SIGBUS` when the lost part is read (as in C).
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_map_file(path: Obj) -> Obj {
    // SAFETY: `path` is live; the state is a fresh allocation owned by the
    // object; the descriptor is closed before returning.
    unsafe {
        let m = sys::malloc(size_of::<Mapping>()) as *mut Mapping;
        if m.is_null() {
            io::fatal(b"out of memory", 251);
        }
        m.write(Mapping {
            ptr: core::ptr::null_mut(),
            len: 0,
        });
        let obj = foreign(m as *mut c_void, mapping_finalize);
        let p = match CBuf::of(path) {
            Ok(p) => p,
            Err(_) => {
                set_errno(EINVAL);
                return obj;
            }
        };
        let fd = sys::open(p.0, sys::O_RDONLY | sys::O_CLOEXEC);
        if fd < 0 {
            return obj;
        }
        let mut st: sys::stat_t = core::mem::zeroed();
        if sys::fstat(fd, &mut st) < 0 {
            let e = errno();
            close(fd);
            set_errno(e);
            return obj;
        }
        let len = st.st_size.max(0) as usize;
        if len > 0 {
            let q = sys::mmap(
                core::ptr::null_mut(),
                len,
                sys::PROT_READ,
                sys::MAP_PRIVATE,
                fd,
                0,
            );
            if q == sys::MAP_FAILED {
                let e = errno();
                close(fd);
                set_errno(e);
                return obj;
            }
            (*m).ptr = q as *mut u8;
            (*m).len = len;
        }
        close(fd);
        set_errno(0);
        obj
    }
}

/// The bytes of the mapping object `h`.
///
/// # Safety
/// `h` must be a live object.
unsafe fn mapped<'a>(h: Obj) -> &'a [u8] {
    // SAFETY: a FOREIGN object has its pointer in word 0 and its finalizer
    // in word 1; a mapping's pointer is its state.
    unsafe {
        if (*h).kind != FOREIGN || *word(h, 1) != mapping_finalize as extern "C" fn(usize) as usize
        {
            io::fatal(b"not a mapping object", 2);
        }
        let m = *word(h, 0) as *const Mapping;
        if (*m).len == 0 {
            &[]
        } else {
            core::slice::from_raw_parts((*m).ptr, (*m).len)
        }
    }
}

/// The range `[from, to)` clamped to `0..len`.
fn clamp(from: i64, to: i64, len: usize) -> (usize, usize) {
    let len = len as i64;
    let a = from.clamp(0, len);
    let b = to.clamp(a, len);
    (a as usize, b as usize)
}

/// The size of the mapping `h` in bytes.
///
/// # Safety
/// `h` must be a live mapping object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_map_len(h: Obj) -> i64 {
    // SAFETY: guaranteed by the caller.
    unsafe { mapped(h).len() as i64 }
}

/// A copy of the bytes `[off, off + n)` of the mapping `h` (clamped to the
/// mapping).
///
/// # Safety
/// `h` must be a live mapping object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_map_slice(h: Obj, off: i64, n: i64) -> Obj {
    // SAFETY: guaranteed by the caller.
    let b = unsafe { mapped(h) };
    let (a, z) = clamp(off, off.saturating_add(n.max(0)), b.len());
    string(&b[a..z])
}

/// The byte at `i` of the mapping `h`, or -1 outside it.
///
/// # Safety
/// `h` must be a live mapping object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_map_byte(h: Obj, i: i64) -> i64 {
    // SAFETY: guaranteed by the caller.
    let b = unsafe { mapped(h) };
    usize::try_from(i)
        .ok()
        .and_then(|i| b.get(i))
        .map_or(-1, |&c| i64::from(c))
}

/// The number of bytes equal to `byte` in `[from, to)` of the mapping `h`.
///
/// # Safety
/// `h` must be a live mapping object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_map_count(h: Obj, byte: i64, from: i64, to: i64) -> i64 {
    // SAFETY: guaranteed by the caller.
    let b = unsafe { mapped(h) };
    let (a, z) = clamp(from, to, b.len());
    let c = byte as u8;
    b[a..z].iter().filter(|&&x| x == c).count() as i64
}

/// The index of the first byte equal to `byte` at or after `from` in the
/// mapping `h`, or -1.
///
/// # Safety
/// `h` must be a live mapping object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_map_find(h: Obj, byte: i64, from: i64) -> i64 {
    // SAFETY: guaranteed by the caller.
    let b = unsafe { mapped(h) };
    let (a, _) = clamp(from, b.len() as i64, b.len());
    let c = byte as u8;
    b[a..]
        .iter()
        .position(|&x| x == c)
        .map_or(-1, |i| (a + i) as i64)
}

// ---- directories, metadata, the working directory ----

/// Runs `f` with `path` as a C string: its result, or `-EINVAL` for a
/// path with a NUL.
///
/// # Safety
/// `path` must be a live BYTES object.
unsafe fn with_path(path: Obj, f: impl FnOnce(*const c_char) -> i64) -> i64 {
    // SAFETY: guaranteed by the caller.
    match unsafe { CBuf::of(path) } {
        Ok(p) => f(p.0),
        Err(e) => e,
    }
}

/// 0, or `-errno` for a C result below 0.
fn status(r: c_int) -> i64 {
    if r < 0 { neg_errno() } else { 0 }
}

/// The names in the directory `path` (without `.` and `..`, in the order
/// of the directory), each followed by a NUL, with `errno` 0; empty with
/// `errno` set on failure.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fs_list(path: Obj) -> Obj {
    // SAFETY: `path` is live; the directory stream is closed before
    // returning; names are NUL-terminated within their entry.
    unsafe {
        let p = match CBuf::of(path) {
            Ok(p) => p,
            Err(_) => {
                set_errno(EINVAL);
                return string(&[]);
            }
        };
        let d = sys::opendir(p.0);
        if d.is_null() {
            return string(&[]);
        }
        let mut names = Buf::new();
        loop {
            set_errno(0);
            let e = sys::readdir(d);
            if e.is_null() {
                break;
            }
            let name = core::ffi::CStr::from_ptr((*e).d_name.as_ptr()).to_bytes();
            if name != b"." && name != b".." {
                names.push(name);
                names.push(&[0]);
            }
        }
        let err = errno();
        sys::closedir(d);
        if err != 0 {
            set_errno(err);
            return string(&[]);
        }
        set_errno(0);
        string(names.bytes())
    }
}

/// The metadata of `path` (of a symbolic link itself unless `follow`):
/// 0 with the results 0 to 3 (size, kind, permissions, modification
/// time), or `-errno`.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fs_stat(path: Obj, follow: i64) -> i64 {
    // SAFETY: `path` is live; stat fills the buffer.
    unsafe {
        with_path(path, |p| {
            let mut st: sys::stat_t = core::mem::zeroed();
            let r = if follow != 0 {
                sys::stat(p, &mut st)
            } else {
                sys::lstat(p, &mut st)
            };
            if r < 0 {
                return neg_errno();
            }
            let kind = match st.st_mode & sys::S_IFMT {
                sys::S_IFREG => 1,
                sys::S_IFDIR => 2,
                sys::S_IFLNK => 3,
                _ => 0,
            };
            set_result(0, st.st_size);
            set_result(1, kind);
            set_result(2, i64::from(st.st_mode & 0o7777));
            set_result(3, st.st_mtime);
            0
        })
    }
}

/// Creates the directory `path` with the permissions `mode` (less the
/// umask): 0 or `-errno`.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fs_mkdir(path: Obj, mode: i64) -> i64 {
    // SAFETY: `path` is live.
    unsafe { with_path(path, |p| status(sys::mkdir(p, mode as u32))) }
}

/// Removes the empty directory `path`: 0 or `-errno`.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fs_rmdir(path: Obj) -> i64 {
    // SAFETY: `path` is live.
    unsafe { with_path(path, |p| status(sys::rmdir(p))) }
}

/// Renames `from` to `to` (replacing it): 0 or `-errno`.
///
/// # Safety
/// `from` and `to` must be live BYTES objects (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_fs_rename(from: Obj, to: Obj) -> i64 {
    // SAFETY: both are live.
    unsafe { with_path(from, |f| with_path(to, |t| status(sys::rename(f, t)))) }
}

/// The working directory, with `errno` 0; empty with `errno` set.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_getcwd() -> Obj {
    let mut size = 4096usize;
    loop {
        // SAFETY: a buffer of `size` bytes, freed before returning.
        unsafe {
            let b = sys::malloc(size) as *mut c_char;
            if b.is_null() {
                set_errno(ENOMEM);
                return string(&[]);
            }
            if !sys::getcwd(b, size).is_null() {
                let s = string(core::ffi::CStr::from_ptr(b).to_bytes());
                sys::free(b as *mut c_void);
                set_errno(0);
                return s;
            }
            let e = errno();
            sys::free(b as *mut c_void);
            if e != ERANGE || size >= 1 << 20 {
                set_errno(e);
                return string(&[]);
            }
            size *= 4;
        }
    }
}

/// Changes the working directory of the process to `path`: 0 or
/// `-errno`.
///
/// # Safety
/// `path` must be a live BYTES object (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_os_chdir(path: Obj) -> i64 {
    // SAFETY: `path` is live.
    unsafe { with_path(path, |p| status(sys::chdir(p))) }
}

/// The host name, with `errno` 0; empty with `errno` set.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_os_hostname() -> Obj {
    let mut b = [0u8; 256];
    // SAFETY: a buffer of its size; the last byte stays NUL.
    unsafe {
        if sys::gethostname(b.as_mut_ptr() as *mut c_char, b.len() - 1) < 0 {
            return string(&[]);
        }
        set_errno(0);
        string(core::ffi::CStr::from_ptr(b.as_ptr() as *const c_char).to_bytes())
    }
}

/// Unused; keeps `EPIPE` documented as the error of a write to a child
/// that stopped reading (dropped by `cirka_os_communicate`).
const _: i32 = EPIPE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::bytes_of;
    use crate::object::cirka_drop;

    fn s(b: &[u8]) -> Obj {
        string(b)
    }

    /// `f` on a runtime string of `b`, dropping it afterwards.
    fn with_str<T>(b: &[u8], f: impl FnOnce(Obj) -> T) -> T {
        let o = s(b);
        let r = f(o);
        // SAFETY: the string made above, used no more.
        unsafe { cirka_drop(o) };
        r
    }

    #[test]
    fn a_child_with_pipes() {
        let _g = crate::test_lock();
        // SAFETY: strings made here; the descriptors are the spawn's.
        unsafe {
            let pid = with_str(
                b"sh\0-c\0read x; echo out $x; echo err >&2; exit 3\0",
                |a| cirka_os_spawn(a, PIPE | (PIPE << 2) | (PIPE << 4)),
            );
            assert!(pid > 0, "{pid}");
            let (i, o, e) = (cirka_os_result(0), cirka_os_result(1), cirka_os_result(2));
            assert!(i >= 0 && o >= 0 && e >= 0);
            let r = with_str(b"in\n", |input| cirka_os_communicate(i, o, e, input));
            let n = cirka_os_result(0) as usize;
            let all = bytes_of(r);
            assert_eq!(&all[..n], b"out in\n");
            assert_eq!(&all[n..], b"err\n");
            cirka_drop(r);
            assert_eq!(cirka_os_wait(pid, 0), 0);
            assert_eq!((cirka_os_result(0), cirka_os_result(1)), (3, 0));
        }
    }

    #[test]
    fn a_child_killed_by_a_signal() {
        let _g = crate::test_lock();
        let pid = with_str(b"sleep\x0010", |a| {
            // SAFETY: a live string.
            unsafe { cirka_os_spawn(a, NULL | (NULL << 2)) }
        });
        assert!(pid > 0);
        assert_eq!(cirka_os_wait(pid, 1), 1, "still running");
        assert_eq!(cirka_os_kill(pid, 15), 0);
        assert_eq!(cirka_os_wait(pid, 0), 0);
        assert_eq!((cirka_os_result(0), cirka_os_result(1)), (-1, 15));
        assert!(cirka_os_wait(pid, 0) < 0, "already waited for");
    }

    #[test]
    fn a_missing_program_and_a_child_that_stops_reading() {
        let _g = crate::test_lock();
        let r = with_str(b"/no/such/program", |a| {
            // SAFETY: a live string.
            unsafe { cirka_os_spawn(a, 0) }
        });
        assert_eq!(r, -2);
        // `true` does not read its input: writing a megabyte to it neither
        // blocks nor ends this process with SIGPIPE.
        // SAFETY: strings made here; the descriptors are the spawn's.
        unsafe {
            let pid = with_str(b"true", |a| cirka_os_spawn(a, PIPE | (PIPE << 2)));
            assert!(pid > 0);
            let big = vec![b'x'; 1 << 20];
            let r = with_str(&big, |input| {
                cirka_os_communicate(cirka_os_result(0), cirka_os_result(1), -1, input)
            });
            assert_eq!(bytes_of(r), b"");
            cirka_drop(r);
            assert_eq!(cirka_os_wait(pid, 0), 0);
            assert_eq!(cirka_os_result(0), 0);
        }
    }

    #[test]
    fn signals_waited_for_and_read_from_a_descriptor() {
        let _g = crate::test_lock();
        let usr1 = 1i64 << (10 - 1);
        let usr2 = 1i64 << (12 - 1);
        assert_eq!(cirka_os_signal_mask(0, usr1), 0);
        assert_eq!(cirka_os_signal_wait(usr1, 0), -i64::from(EAGAIN));
        assert_eq!(cirka_os_raise(10), 0);
        assert_eq!(cirka_os_signal_wait(usr1, 1000), 10);
        assert_eq!(cirka_os_signal_mask(1, usr1), 0);
        let fd = cirka_os_signalfd(usr2);
        assert!(fd >= 0);
        let h = crate::net::cirka_fd_handle(fd);
        // SAFETY: a live descriptor object, dropped at the end.
        unsafe {
            assert_eq!(cirka_os_signalfd_read(h), -i64::from(EAGAIN));
            assert_eq!(cirka_os_raise(12), 0);
            assert_eq!(cirka_os_signalfd_read(h), 12);
            cirka_drop(h);
        }
        assert_eq!(cirka_os_signal_mask(1, usr2), 0);
        assert_eq!(cirka_os_signal_action(11, 1), -i64::from(EINVAL));
        assert_eq!(cirka_os_signal_action(13, 1), 0);
        assert_eq!(cirka_os_signal_action(13, 0), 0);
    }

    /// `sigtimedwait` writes a whole `siginfo_t` of the C library, which
    /// ends where the runtime's ends: the word after it is left as it was
    /// (BUG-0025).
    #[test]
    fn sigtimedwait_stays_within_siginfo() {
        let _g = crate::test_lock();
        #[repr(C)]
        struct Guarded {
            info: sys::siginfo_t,
            canary: u64,
        }
        const CANARY: u64 = 0x5a5a_5a5a_5a5a_5a5a;
        let usr1 = 1i64 << (10 - 1);
        let set = sigset(usr1);
        let zero = sys::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        assert_eq!(cirka_os_signal_mask(0, usr1), 0);
        assert_eq!(cirka_os_raise(10), 0);
        // SAFETY: a zeroed information buffer followed by the canary.
        let (r, canary) = unsafe {
            let mut g: Guarded = core::mem::zeroed();
            g.canary = CANARY;
            let r = sys::sigtimedwait(&set, &mut g.info, &zero);
            (r, core::ptr::read_volatile(&g.canary))
        };
        assert_eq!(cirka_os_signal_mask(1, usr1), 0);
        assert_eq!(r, 10);
        assert_eq!(canary, CANARY);
    }

    #[test]
    fn files_mapped_listed_and_described() {
        let _g = crate::test_lock();
        let dir = format!("/tmp/cirka-os-test-{}\0", std::process::id());
        let d = dir.trim_end_matches('\0').as_bytes().to_vec();
        let file = [&d[..], b"/lines.txt"].concat();
        let moved = [&d[..], b"/moved.txt"].concat();
        // SAFETY: strings made here.
        unsafe {
            assert_eq!(with_str(&d, |p| cirka_fs_mkdir(p, 0o755)), 0);
            assert_eq!(with_str(&d, |p| cirka_fs_mkdir(p, 0o755)), -17);
            std::fs::write(std::str::from_utf8(&file).unwrap(), "a\nbb\nccc\n").unwrap();
            let m = with_str(&file, |p| cirka_os_map_file(p));
            assert_eq!(errno(), 0);
            assert_eq!(cirka_os_map_len(m), 9);
            assert_eq!(cirka_os_map_count(m, 10, 0, 100), 3);
            assert_eq!(cirka_os_map_find(m, 10, 2), 4);
            assert_eq!(cirka_os_map_find(m, 122, 0), -1);
            assert_eq!(cirka_os_map_byte(m, 2), i64::from(b'b'));
            assert_eq!(cirka_os_map_byte(m, 9), -1);
            let sl = cirka_os_map_slice(m, 2, 2);
            assert_eq!(bytes_of(sl), b"bb");
            cirka_drop(sl);
            cirka_drop(m);
            let missing = with_str(b"/no/such/file", |p| cirka_os_map_file(p));
            assert_eq!(errno(), 2);
            assert_eq!(cirka_os_map_len(missing), 0);
            cirka_drop(missing);
            assert_eq!(with_str(&file, |p| cirka_fs_stat(p, 1)), 0);
            assert_eq!((cirka_os_result(0), cirka_os_result(1)), (9, 1));
            assert_eq!(with_str(&d, |p| cirka_fs_stat(p, 1)), 0);
            assert_eq!(
                (cirka_os_result(1), cirka_os_result(2)),
                (2, 0o755 & !0o022)
            );
            let r = s(&file);
            let t = s(&moved);
            assert_eq!(cirka_fs_rename(r, t), 0);
            cirka_drop(r);
            cirka_drop(t);
            let names = with_str(&d, |p| cirka_fs_list(p));
            assert_eq!(bytes_of(names), b"moved.txt\0");
            cirka_drop(names);
            assert_eq!(with_str(&d, |p| cirka_fs_rmdir(p)), -39, "not empty");
            std::fs::remove_file(std::str::from_utf8(&moved).unwrap()).unwrap();
            assert_eq!(with_str(&d, |p| cirka_fs_rmdir(p)), 0);
            assert_eq!(with_str(&d, |p| cirka_fs_stat(p, 1)), -2);
            let none = with_str(&d, |p| cirka_fs_list(p));
            assert_eq!((bytes_of(none), errno()), (&b""[..], 2));
            cirka_drop(none);
        }
    }

    #[test]
    fn the_working_directory_and_the_host_name() {
        let _g = crate::test_lock();
        let cwd = cirka_os_getcwd();
        // SAFETY: strings made by the runtime, dropped here.
        unsafe {
            assert_eq!(
                bytes_of(cwd),
                std::env::current_dir()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .as_bytes()
            );
            cirka_drop(cwd);
            let h = cirka_os_hostname();
            assert!(!bytes_of(h).is_empty());
            cirka_drop(h);
        }
    }
}
