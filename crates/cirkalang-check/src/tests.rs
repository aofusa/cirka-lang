use crate::check;
use cirka_diagnostics::SourceMap;
use cirkalang_resolve::{MemLoader, Options, resolve_program};
use std::collections::HashMap;

const PRELUDE: &str = r#"module Prelude

infixl 6 +, -
infixr 5 ::

prim Int : Type
prim String : Type
prim IO : Type -> Type

data Bool = False | True
data Unit = MkUnit
data Nat = Z | S Nat
data List a = Nil | a :: List a

prim primAddInt : Int -> Int -> Int
prim primSubInt : Int -> Int -> Int
prim primLeInt : Int -> Int -> Bool
prim panic : {a : LType} -> String -> a

(+) : Int -> Int -> Int
x + y = primAddInt x y

(-) : Int -> Int -> Int
x - y = primSubInt x y
"#;

/// The codes of the totality errors of `src`.
fn totality(src: &str) -> Vec<String> {
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
    let el = cirkalang_elab::elaborate(&rp);
    let Some(prog) = el.program else {
        panic!(
            "elaboration failed: {:?}",
            el.diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    };
    check(&rp, &prog, None)
        .iter()
        .map(|d| format!("{}: {}", d.code.clone().unwrap_or_default(), d.message))
        .collect()
}

#[test]
fn structural_recursion_is_total() {
    let e = totality(
        "module Main\n\
         total\n\
         ack : Nat -> Nat -> Nat\n\
         ack Z n = S n\n\
         ack (S m) Z = ack m (S Z)\n\
         ack (S m) (S n) = ack m (ack (S m) n)\n\
         total\n\
         len : List a -> Int\n\
         len [] = 0\n\
         len (_ :: xs) = 1 + len xs\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn permuted_arguments_terminate() {
    // p (S x) y = p y x: the size-change graphs compose to a decrease.
    let e = totality(
        "module Main\n\
         total\n\
         p : Nat -> Nat -> Nat\n\
         p Z y = y\n\
         p (S x) y = p y x\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn mutual_recursion_terminates() {
    let e = totality(
        "module Main\n\
         total\n\
         even : Nat -> Bool\n\
         even Z = True\n\
         even (S n) = odd n\n\
         odd : Nat -> Bool\n\
         odd Z = False\n\
         odd (S n) = even n\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn non_terminating_functions_are_rejected() {
    let e = totality(
        "module Main\n\
         total\n\
         f : Nat -> Nat\n\
         f n = f n\n",
    );
    assert_eq!(e, vec!["E0502: cannot show that `f` terminates"]);
    let e = totality(
        "module Main\n\
         total\n\
         g : Nat -> Nat -> Nat\n\
         g Z y = y\n\
         g (S x) y = g (S y) x\n",
    );
    assert_eq!(e, vec!["E0502: cannot show that `g` terminates"]);
}

#[test]
fn total_functions_only_use_total_functions() {
    let e = totality(
        "module Main\n\
         partial\n\
         pred : Nat -> Nat\n\
         pred (S n) = n\n\
         total\n\
         two : Nat -> Nat\n\
         two n = pred (pred n)\n",
    );
    assert_eq!(e, vec!["E0504: `two` is declared `total` but is not total"]);
    let e = totality(
        "module Main\n\
         total\n\
         boom : Nat -> Nat\n\
         boom n = panic \"no\"\n",
    );
    assert_eq!(
        e,
        vec!["E0504: `boom` is declared `total` but is not total"]
    );
}

#[test]
fn covering_functions_are_not_checked() {
    let e = totality(
        "module Main\n\
         f : Nat -> Nat\n\
         f n = f n\n\
         covering\n\
         g : Nat -> Nat\n\
         g n = g n\n",
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn non_positive_types_are_not_total() {
    let e = totality(
        "module Main\n\
         data Bad = MkBad (Bad -> Int)\n\
         total\n\
         app : Bad -> Int\n\
         app (MkBad f) = f (MkBad f)\n",
    );
    assert_eq!(e, vec!["E0504: `app` is declared `total` but is not total"]);
}

#[test]
fn functions_in_types_must_be_total() {
    let e = totality(
        "module Main\n\
         data T (n : Nat) = MkT\n\
         bad : Nat -> Nat\n\
         bad n = bad n\n\
         use : T (bad Z) -> Int\n\
         use _ = 0\n",
    );
    assert_eq!(e, vec!["E0505: `bad` is used in a type but is not total"]);
}

#[test]
fn int_recursion_needs_z3() {
    // With Z3, the decreasing `Int` parameter is found automatically.
    if cirka_smt::Solver::from_env().is_none() {
        eprintln!("z3 not found: skipped");
        return;
    }
    let e = totality(
        "module Main\n\
         total\n\
         down : Int -> Int\n\
         down n = case primLeInt n 0 of\n  True => 0\n  False => down (n - 1)\n",
    );
    assert!(e.is_empty(), "{e:?}");
    let e = totality(
        "module Main\n\
         total\n\
         up : Int -> Int\n\
         up n = case primLeInt n 0 of\n  True => 0\n  False => up (n + 1)\n",
    );
    assert_eq!(e, vec!["E0502: cannot show that `up` terminates"]);
}
