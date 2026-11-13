use crate::*;
use cirka_diagnostics::SourceMap;
use cirkalang_resolve::{MemLoader, Options, resolve_program};
use std::collections::HashMap;

pub(crate) const PRELUDE: &str = r#"module Prelude

infixl 6 +, -
infixr 5 ::
infix 4 ==
infixl 1 >>=, >>

prim Int : Type
prim UInt : Type
prim Float : Type
prim Char : Type
prim String : Type
prim IO : Type -> Type
prim Unique : Type -> LType

data Bool = False | True
data Unit = MkUnit
data Nat = Z | S Nat
data Pair a b = MkPair a b
data List a = Nil | a :: List a

prim primAddInt : Int -> Int -> Int
prim primSubInt : Int -> Int -> Int
prim primEqInt : Int -> Int -> Bool
prim primPure : a -> IO a
prim primBind : IO a -> (a -> IO b) -> IO b
prim printInt : Int -> IO ()
prim unique : a -> Unique a
prim freeze : (1 u : Unique a) -> a

trait Drop (a : LType) where
  drop : (1 x : a) -> ()

prim dropFree : {a : LType} -> (1 x : a) -> ()

pure : a -> IO a
pure x = primPure x

negate : Int -> Int
negate x = primSubInt 0 x

(+) : Int -> Int -> Int
x + y = primAddInt x y

(-) : Int -> Int -> Int
x - y = primSubInt x y

(==) : Int -> Int -> Bool
x == y = primEqInt x y

(>>=) : IO a -> (a -> IO b) -> IO b
m >>= f = primBind m f

(>>) : IO a -> IO b -> IO b
m >> k = primBind m (\_ => k)
"#;

fn elab(src: &str) -> (Elaborated, SourceMap) {
    let mut files: HashMap<String, String> = HashMap::new();
    files.insert("Prelude".into(), PRELUDE.into());
    let mut sm = SourceMap::new();
    let id = sm.add("Main.cirka", src.to_string());
    let mut loader = MemLoader(files);
    let rp = match resolve_program(&mut sm, id, src, &mut loader, &Options::default()) {
        Ok(p) => p,
        Err(e) => panic!(
            "resolve errors: {:?}",
            e.iter().map(|d| &d.message).collect::<Vec<_>>()
        ),
    };
    (elaborate(&rp), sm)
}

fn codes(e: &Elaborated) -> Vec<String> {
    e.diags
        .iter()
        .map(|d| format!("{}: {}", d.code.clone().unwrap_or_default(), d.message))
        .collect()
}

fn ok(src: &str) -> tt::Program {
    let (e, _) = elab(src);
    let errs: Vec<String> = codes(&e);
    match e.program {
        Some(p) => p,
        None => panic!("elaboration failed:\n{}", errs.join("\n")),
    }
}

fn fails(src: &str, code: &str) {
    let (e, _) = elab(src);
    let cs = codes(&e);
    assert!(
        e.program.is_none(),
        "expected {code}, but it succeeded: {cs:?}"
    );
    assert!(
        cs.iter().any(|c| c.starts_with(code)),
        "expected {code}, got {cs:?}"
    );
}

#[test]
fn prelude_elaborates() {
    let p = ok("main : IO ()\nmain = printInt 1\n");
    let out = tt::print_program(&p);
    assert!(
        out.contains("Prelude.+ = \\x => \\y => primAddInt x y"),
        "{out}"
    );
}

const VECT: &str = r#"
data Vect : (0 n : Nat) -> (a : Type) -> Type where
  VNil : Vect Z a
  VCons : a -> Vect n a -> Vect (S n) a

plus : Nat -> Nat -> Nat
plus Z m = m
plus (S k) m = S (plus k m)

append : Vect n a -> Vect m a -> Vect (plus n m) a
append VNil ys = ys
append (VCons x xs) ys = VCons x (append xs ys)

vhead : Vect (S n) a -> a
vhead (VCons x _) = x

vlength : Vect n a -> Int
vlength VNil = 0
vlength (VCons _ xs) = 1 + vlength xs

three : Vect 3 Int
three = append (VCons 1 VNil) (VCons 2 (VCons 3 VNil))

main : IO ()
main = do
  printInt (vlength three)
  printInt (vhead three)
"#;

#[test]
fn dependent_vectors() {
    let p = ok(VECT);
    let out = tt::print_program(&p);
    assert!(out.contains("Main.append : {n : Nat} -> {a : Type} -> {m : Nat} -> Vect n a -> Vect m a -> Vect (plus n m) a"), "{out}");
    // `vhead` has no VNil branch: it is absurd.
    assert!(
        out.contains(
            "Main.vhead = \\{n} => \\{a} => \\x0 => case x0 of { VCons {a1} {n1} x v3 => x }"
        ),
        "{out}"
    );
}

#[test]
fn wrong_length_is_a_type_error() {
    let src = VECT.replace("three : Vect 3 Int", "three : Vect 2 Int");
    fails(&src, "E1200");
}

#[test]
fn ownership_ok() {
    ok(r#"
useOnce : (1 x : Unique Int) -> Int
useOnce x = freeze x

swap : (1 p : (a, b)) -> (b, a)
swap (x, y) = (y, x)

dropIt : (1 x : Unique Int) -> Int
dropIt x = 0

both : (1 x : Unique Int) -> Bool -> Int
both x b = if b then freeze x else 0

main : IO ()
main = printInt (useOnce (unique 1))
"#);
}

#[test]
fn ownership_errors() {
    fails(
        "twice : (1 x : Unique Int) -> Int\ntwice x = freeze x + freeze x\nmain : IO ()\nmain = pure ()\n",
        "E0303",
    );
    fails(
        "shared : Unique Int -> Int\nshared x = freeze x\nmain : IO ()\nmain = pure ()\n",
        "E0301",
    );
    fails(
        "leak : {a : LType} -> (1 x : a) -> Int\nleak x = 0\nmain : IO ()\nmain = pure ()\n",
        "E0302",
    );
    fails(
        "bad : (0 n : Int) -> Int\nbad n = n\nmain : IO ()\nmain = pure ()\n",
        "E0304",
    );
    fails(
        "apply2 : (Int -> Int) -> Int\napply2 f = f (f 0)\ncap : (1 x : Unique Int) -> Int\ncap x = apply2 (\\y => freeze x)\nmain : IO ()\nmain = pure ()\n",
        "E0305",
    );
    fails(
        "br : {a : LType} -> (1 x : a) -> (1 k : (1 y : a) -> Int) -> Bool -> Int\nbr x k b = if b then k x else 0\nmain : IO ()\nmain = pure ()\n",
        "E0306",
    );
}

#[test]
fn type_errors() {
    fails("f : Int\nf = True\nmain : IO ()\nmain = pure ()\n", "E1200");
    fails(
        "g : Bool -> Int\ng True = 1\nmain : IO ()\nmain = pure ()\n",
        "E1230",
    );
    fails(
        "h : Int -> Int\nh x = ?goal\nmain : IO ()\nmain = pure ()\n",
        "E1211",
    );
    fails("k : Int\nk = 1 2\nmain : IO ()\nmain = pure ()\n", "E1204");
    fails("main : Int\nmain = 1\n", "E1200");
    fails(
        "data Bad : Type where\n  MkBad : Unique Int -> Bad\nmain : IO ()\nmain = pure ()\n",
        "E1203",
    );
    fails(
        "data T : Type where\n  MkT : Int\nmain : IO ()\nmain = pure ()\n",
        "E1201",
    );
    fails(
        "f : (1 t : Type) -> Int\nf t = 0\nmain : IO ()\nmain = pure ()\n",
        "E1240",
    );
    let (e, _) = elab("g : Bool -> Int\ng _ = 1\ng True = 2\nmain : IO ()\nmain = pure ()\n");
    assert!(e.program.is_some());
    assert!(
        codes(&e).iter().any(|c| c.starts_with("W1231")),
        "{:?}",
        codes(&e)
    );
}

#[test]
fn literals_and_sugar() {
    ok(r#"
isZero : Int -> Bool
isZero 0 = True
isZero _ = False

pred : Nat -> Nat
pred 0 = 0
pred (S n) = n

len : List a -> Int
len [] = 0
len (_ :: xs) = 1 + len xs

xs : List Int
xs = [1, 2, -3]

fst : (a, b) -> a
fst (x, _) = x

main : IO ()
main = do
  n <- pure (len xs)
  let m = n + 1
  printInt (fst (m, True))
"#);
}

#[test]
fn lint_catches_bad_terms() {
    use tt::*;
    let mut p = ok("main : IO ()\nmain = pure ()\n");
    let g = p.main.unwrap();
    // Replace main's body with an ill-typed term (an Int literal).
    if let Global::Def { body, .. } = &mut p.globals[g.0 as usize] {
        *body = Some(Term::Lit(Lit::Int(1)));
    }
    assert!(lint::lint_program(&p).is_err());
}

#[test]
fn spec_example() {
    ok(r#"
data Tree = Leaf | Node Tree Tree

make : Int -> Tree
make 0 = Node Leaf Leaf
make d = Node (make (d - 1)) (make (d - 1))

check : Tree -> Int
check Leaf = 0
check (Node l r) = 1 + check l + check r

data Vect : (0 n : Nat) -> (a : Type) -> Type where
  VNil  : Vect 0 a
  VCons : a -> Vect n a -> Vect (S n) a

swap : (1 p : (a, b)) -> (b, a)
swap (x, y) = (y, x)

data Box : LType where
  MkBox : (1 u : Unique Int) -> Box

open : (1 b : Box) -> Int
open (MkBox u) = freeze u

main : IO ()
main = do
  printInt (check (make 10))
  printInt (open (MkBox (unique 3)))
"#);
}

/// Regression: a solved meta applied to arguments must be beta-reduced
/// when zonked (it left lambdas with error annotations).
#[test]
fn zonk_reduces_applied_metas() {
    ok(
        "foldl : (b -> a -> b) -> b -> List a -> b\nfoldl f z Nil = z\nfoldl f z (x :: xs) = foldl f (f z x) xs\nlength : List a -> Int\nlength xs = foldl (\\n, _ => n + 1) 0 xs\nmain : IO ()\nmain = pure ()\n",
    );
}

#[test]
fn resource_free_arguments_and_empty_matches() {
    // `freeze u` is an Int: passing it to the ω arguments of `+` uses `u`
    // once (ADR-030).
    ok(
        "f : (1 u : Unique Int) -> Int\nf u = freeze u + 1\ng : (1 u : Unique Int) -> Int\ng u = let n = freeze u in n + n\nmain : IO ()\nmain = pure ()\n",
    );
    // ... but a closure is not resource-free.
    fails(
        "k : (1 u : Unique Int) -> Int -> Int\nk u = \\x => x + freeze u\nh : (Int -> Int) -> Int\nh f = f (f 0)\nbad : (1 u : Unique Int) -> Int\nbad u = h (k u)\nmain : IO ()\nmain = pure ()\n",
        "E0303",
    );
    // An empty match on an empty type, and on an impossible index.
    ok(
        "data Void : LType where\nabsurd : (1 v : Void) -> a\nabsurd v = case v of {}\nmain : IO ()\nmain = pure ()\n",
    );
}

/// The argument type `List ?a` is resource-free once `?a := Int` is solved
/// by checking the argument itself.
#[test]
fn resource_free_after_solving_metas() {
    ok(
        "len : List a -> Int\nlen _ = 0\ndata Box : LType where\n  MkBox : (1 u : Unique Int) -> (1 xs : List Int) -> Box\nf : (1 b : Box) -> Int\nf (MkBox u xs) = len xs\nmain : IO ()\nmain = pure ()\n",
    );
    fails(
        "len : List a -> Int\nlen _ = 0\ng : (1 u : Unique Int) -> Int\ng u = len [u]\nmain : IO ()\nmain = pure ()\n",
        "E0303",
    );
}

#[test]
fn shift_moves_free_variables() {
    use std::rc::Rc;
    use tt::*;
    // \x => f x y   (y free: index 1 under the binder)
    let t = Term::Lam(
        "x".into(),
        Mult::Many,
        Icit::Expl,
        Rc::new(Term::Type),
        Rc::new(Term::App(
            Rc::new(Term::App(
                Rc::new(Term::Var(2)),
                Rc::new(Term::Var(0)),
                Icit::Expl,
            )),
            Rc::new(Term::Var(1)),
            Icit::Expl,
        )),
    );
    let s = shift(&t, 0, 1);
    let Term::Lam(_, _, _, _, b) = &s else {
        panic!("{s:?}")
    };
    let Term::App(f, y, _) = &**b else {
        panic!("{b:?}")
    };
    assert_eq!(**y, Term::Var(2));
    let Term::App(g, x, _) = &**f else {
        panic!("{f:?}")
    };
    assert_eq!(**x, Term::Var(0));
    assert_eq!(**g, Term::Var(3));
}

const TOK: &str = "data Tok : LType where\n  MkTok : Int -> Tok\nuse : (1 t : Tok) -> Int -> Int\nuse t k = case t of\n  MkTok n => n + k\nuseIO : (1 t : Tok) -> IO ()\nuseIO t = printInt (use t 0)\n";

/// A value whose type does not show the linear value it holds is linear
/// itself, and a top-level value cannot hold one (BUG-0015).
#[test]
fn holders_are_linear() {
    fails(
        &format!("{TOK}main : IO ()\nmain = let g = use (MkTok 3) in printInt (g 1 + g 2)\n"),
        "E0303",
    );
    ok(&format!(
        "{TOK}main : IO ()\nmain = let g = use (MkTok 3) in printInt (g 1)\n"
    ));
    fails(
        &format!("{TOK}tok : Tok\ntok = MkTok 1\nmain : IO ()\nmain = pure ()\n"),
        "E0301",
    );
    fails(
        &format!("{TOK}f : Int -> Int\nf = use (MkTok 1)\nmain : IO ()\nmain = pure ()\n"),
        "E0305",
    );
    // An action creates the value at each run.
    let p = ok(&format!(
        "{TOK}act : IO ()\nact = useIO (MkTok 1)\nmain : IO ()\nmain = act >> act\n"
    ));
    let out = tt::print_program(&p);
    assert!(
        out.contains("act = primBind (primPure MkUnit) (\\_ => useIO (MkTok 1))"),
        "{out}"
    );
}

/// Values without resources may be used many times (spec-v0.0.2.md §5.1).
#[test]
fn resource_free_binders_are_copied() {
    ok("double : (1 n : Int) -> Int\ndouble n = n + n\nmain : IO ()\nmain = pure ()\n");
    fails(
        &format!(
            "{TOK}twice : (1 t : Tok) -> Int\ntwice t = use t (use t 0)\nmain : IO ()\nmain = pure ()\n"
        ),
        "E0303",
    );
}

/// `drop` is inserted before the rest of a path that does not use a value,
/// after the parameters that follow (the arity stays) (spec-v0.0.2.md §5.3).
#[test]
fn drops_keep_the_arity() {
    let p = ok(
        "data Buf : LType where\n  MkBuf : Int -> Buf\nimpl Drop Buf where\n  drop (MkBuf _) = MkUnit\nignore : (1 b : Buf) -> Int -> Int\nignore b n = n\npick : Bool -> (1 b : Buf) -> Int\npick c b = case c of\n  True => (case b of\n    MkBuf n => n)\n  False => 0\nmain : IO ()\nmain = pure ()\n",
    );
    let out = tt::print_program(&p);
    assert!(
        out.contains("ignore = \\b => \\n => let drop$ = drop b in n"),
        "{out}"
    );
    assert!(out.contains("False => let drop$ = drop b in 0"), "{out}");
}

/// A holder of a value with a finalizer that is never used drops it
/// without the finalizer: a leak, reported as a warning (spec-v0.0.2.md
/// §5.5).
#[test]
fn unused_holders_of_finalized_values_warn() {
    const BUF: &str = "data Buf : LType where\n  MkBuf : Int -> Buf\nimpl Drop Buf where\n  drop (MkBuf _) = MkUnit\nuse : (1 b : Buf) -> Int -> Int\nuse (MkBuf n) k = n + k\n";
    let (e, _) = elab(&format!(
        "{BUF}f : Int -> Int\nf n = let g = use (MkBuf n) in n\nmain : IO ()\nmain = pure ()\n"
    ));
    assert!(e.program.is_some(), "{:?}", codes(&e));
    assert!(
        codes(&e).iter().any(|c| c.starts_with("W0308")),
        "{:?}",
        codes(&e)
    );
    // Using the holder runs the finalizer: no warning.
    let (e, _) = elab(&format!(
        "{BUF}f : Int -> Int\nf n = let g = use (MkBuf n) in g 1\nmain : IO ()\nmain = pure ()\n"
    ));
    assert!(codes(&e).is_empty(), "{:?}", codes(&e));
    // A holder of a value without a finalizer is freed by Perceus.
    let (e, _) = elab(&format!(
        "{TOK}f : Int -> Int\nf n = let g = use (MkTok n) in n\nmain : IO ()\nmain = pure ()\n"
    ));
    assert!(codes(&e).is_empty(), "{:?}", codes(&e));
}
