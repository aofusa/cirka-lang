use crate::{Contracts, check, check_with};
use cirka_diagnostics::SourceMap;
use cirkalang_resolve::{MemLoader, Options, resolve_program};
use std::collections::{HashMap, HashSet};

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
prim subsetOf : (a : Type) -> (a -> Bool) -> Type
prim primRefine : {a : Type} -> (0 p : a -> Bool) -> (0 site : Int) -> a -> a

(+) : Int -> Int -> Int
x + y = primAddInt x y

(-) : Int -> Int -> Int
x - y = primSubInt x y
"#;

/// The codes of the totality errors of `src`.
fn totality(src: &str) -> Vec<String> {
    checked(src, &[], &HashSet::new())
}

/// The diagnostics of `src` whose imports `modules` are given, without
/// checking the obligations of the modules in `verified`.
fn checked(src: &str, modules: &[(&str, &str)], verified: &HashSet<String>) -> Vec<String> {
    let mut files: HashMap<String, String> = HashMap::new();
    files.insert("Prelude".into(), PRELUDE.into());
    for (name, text) in modules {
        files.insert(name.to_string(), text.to_string());
    }
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
    let diags = if verified.is_empty() {
        check(&rp, &prog, None)
    } else {
        check_with(&rp, &prog, None, Contracts::Verify, verified).diags
    };
    diags
        .iter()
        .map(|d| format!("{}: {}", d.code.clone().unwrap_or_default(), d.message))
        .collect()
}

/// A trusted `total` (spec-v0.0.5.md §4.2) is assumed: the body of `spin`
/// is not looked at, and functions using it are total.
#[test]
fn trusted_totality_is_assumed() {
    let src = |trusted: &str| {
        format!(
            "module Main\n\
             {trusted}total\n\
             spin : Nat -> Nat\n\
             spin n = spin n\n\
             total\n\
             next : Nat -> Nat\n\
             next n = S (spin n)\n"
        )
    };
    let e = totality(&src("trusted "));
    assert!(e.is_empty(), "{e:?}");
    let e = totality(&src(""));
    assert_eq!(
        e,
        vec![
            "E0502: cannot show that `spin` terminates",
            "E0504: `next` is declared `total` but is not total"
        ]
    );
}

/// The obligations of the libraries are verified by their own tests
/// (spec-v0.0.5.md §4.3): those of a module given as verified are not
/// checked.
#[test]
fn obligations_of_verified_modules_are_not_checked() {
    let lib = "module Lib\n\
               small : {x : Int | primLeInt x 3} -> Int\n\
               small x = x\n\
               five : Int\n\
               five = small 5\n";
    let main = "module Main\nimport Lib\nsix : Int\nsix = five + 1\n";
    let e = checked(main, &[("Lib", lib)], &HashSet::new());
    assert!(e.iter().any(|d| d.starts_with("E0501")), "{e:?}");
    let e = checked(main, &[("Lib", lib)], &HashSet::from(["Lib".to_string()]));
    assert!(e.is_empty(), "{e:?}");
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
