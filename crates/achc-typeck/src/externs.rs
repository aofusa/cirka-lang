//! The rules of extern blocks (FEAT-0130, 08-ffi.md §7.4) on the types of
//! their functions once type synonyms are expanded, which the renamer (that
//! adds the scopes of borrowed pointers) cannot see through.

use crate::types::*;

const OWNED: &str = "Unsafe.Extern.OwnedPtr";
const BORROWED: &str = "Unsafe.Extern.BorrowedPtr";
const BORROW_IO: &str = "Unsafe.Extern.BorrowIO";
const BORROW: &str = "Unsafe.Borrow.Borrow";
const MAYBE: &str = "Cirka.Base.Maybe";

/// `OwnedPtr T`, also under `Maybe` (`Nullable`).
fn owned(t: &Ty) -> bool {
    match t.spine() {
        (Ty::Con(h), a) if h == OWNED && a.len() == 1 => true,
        (Ty::Con(h), a) if h == MAYBE && a.len() == 1 => owned(a[0]),
        _ => false,
    }
}

/// The scopes `r` of the `BorrowedPtr r T` in a type.
fn scopes<'a>(t: &'a Ty, out: &mut Vec<&'a Ty>) {
    if let (Ty::Con(h), a) = t.spine()
        && h == BORROWED
        && a.len() == 2
    {
        out.push(a[0]);
    }
    match t {
        Ty::App(f, x) | Ty::Fun(f, x) => {
            scopes(f, out);
            scopes(x, out);
        }
        Ty::Forall(_, b) => scopes(b, out),
        Ty::Var(_) | Ty::Meta(_) | Ty::Con(_) => {}
    }
}

/// What is wrong with the type of a function of an extern block: an
/// `OwnedPtr` parameter that is not linear, an `OwnedPtr` result outside
/// `IO` (each call makes a new resource, which a shared or moved pure call
/// would duplicate), or a borrowed pointer whose scope is not the one of
/// the borrow monad of the result (it could leave the borrow). The message
/// and a note.
pub(crate) fn violation(t: &Ty) -> Option<(&'static str, &'static str)> {
    let mut t = t;
    while let Ty::Forall(_, b) = t {
        t = b;
    }
    let mut params = Vec::new();
    loop {
        if let Ty::Fun(a, r) = t {
            params.push((false, &**a));
            t = r;
        } else if let Some((m, a, r)) = as_mul_fun(t) {
            params.push((*m == mult_one(), a));
            t = r;
        } else {
            break;
        }
    }
    if params.iter().any(|(lin, p)| owned(p) && !lin) {
        return Some((
            "an `OwnedPtr` parameter of a C function must be linear",
            "the C function takes the pointer over: write `OwnedPtr T %1 -> ...`",
        ));
    }
    // Whether a call is an action, the scope of the borrow monad of the
    // result, and the value of the result.
    let (action, scope, value) = match t.spine() {
        (Ty::Con(h), a) if *h == prim("IO") && a.len() == 1 => (true, None, a[0]),
        (Ty::Con(h), a) if h == BORROW_IO && a.len() == 2 => (true, Some(a[0]), a[1]),
        (Ty::Con(h), a) if h == BORROW && a.len() == 2 => (false, Some(a[0]), a[1]),
        _ => (false, None, t),
    };
    if !action && owned(value) {
        return Some((
            "a C function that returns an `OwnedPtr` must be an `IO` action",
            "each call makes a new resource: write `... -> IO (OwnedPtr T)`",
        ));
    }
    let mut ss = Vec::new();
    for (_, p) in &params {
        scopes(p, &mut ss);
    }
    scopes(value, &mut ss);
    let scoped = match scope {
        Some(r @ Ty::Var(_)) => ss.iter().all(|s| *s == r),
        _ => ss.is_empty(),
    };
    if !scoped {
        return Some((
            "the borrowed pointers of a C function must have the scope of its result",
            "write `BorrowedPtr T` in the extern block, not through a type synonym: \
             the compiler adds the scope `r` and returns in `BorrowIO r` / `Borrow r`",
        ));
    }
    None
}
