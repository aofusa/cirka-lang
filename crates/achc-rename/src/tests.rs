use crate::*;
use achc_syntax::print::print_module;

const PRIM_SRC: &str = "{-# LANGUAGE NoImplicitPrelude #-}
module Cirka.Prim where
data Int
data Bool = False | True
foreign import prim \"int_add\" primIntAdd :: Int -> Int -> Int
";

const PRELUDE: &str = "{-# LANGUAGE NoImplicitPrelude #-}
module Prelude (module Prelude, module Cirka.Prim) where
import Cirka.Prim
infixl 6 +, -
infixr 9 .
infixr 0 $
(+) :: Int -> Int -> Int
x + y = primIntAdd x y
(-) :: Int -> Int -> Int
x - y = x
negate :: Int -> Int
negate x = x
(.) :: (b -> c) -> (a -> b) -> a -> c
(f . g) x = f (g x)
($) :: (a -> b) -> a -> b
f $ x = f x
data Maybe a = Nothing | Just a
map :: (a -> b) -> [a] -> [b]
map f [] = []
map f (x:xs) = f x : map f xs
";

fn rn(src: &str, extra: &[(&str, &str)]) -> Result<Program, Vec<String>> {
    let mut mods = vec![("Cirka.Prim", PRIM_SRC), ("Prelude", PRELUDE)];
    mods.extend_from_slice(extra);
    let mut sm = SourceMap::new();
    rename_source(&mut sm, src, &mods, &Options::default()).map_err(|es| {
        es.iter()
            .map(|d| format!("{}: {}", d.code.clone().unwrap_or_default(), d.message))
            .collect()
    })
}

fn main_of(p: &Program) -> String {
    print_module(&p.modules.last().unwrap().ast)
}

#[test]
fn resolves_and_reassociates() {
    let p = rn(
        "f x = negate x + 1 - x * 2\ng = map (+ 1) . map (2 -) $ [1, 2]\nh (x:y:_) = - x\nh _ = 0\n*x* = 3\ninfixl 7 *\n(*) :: Int -> Int -> Int\na * b = a\n",
        &[],
    );
    // `*x* = 3` is not valid; check the errors instead below.
    assert!(p.is_err());
    let p = rn(
        "infixl 7 *\n(*) :: Int -> Int -> Int\na * b = a\nf x = negate x + 1 - x * 2\ng = map (+ 1) . map (2 -) $ [1, 2]\nh (x:y:_) = - x\nh _ = 0\n",
        &[],
    )
    .unwrap();
    let out = main_of(&p);
    assert!(
        out.contains("((Prelude.- ((Prelude.+ (Prelude.negate x#3)) 1)) ((Main.* x#3) 2))"),
        "{out}"
    );
    assert!(
        out.contains("((Prelude.$ ((Prelude.. (Prelude.map (section `Prelude.+` 1))) (Prelude.map (section 2 `Prelude.-`)))) [1 2])"),
        "{out}"
    );
    assert!(
        out.contains("(fun h ((Cirka.Prim.: x#4 (Cirka.Prim.: y#5 _)))"),
        "{out}"
    );
    assert!(out.contains("(Prelude.negate x#4)"), "{out}");
}

#[test]
fn scopes_and_shadowing() {
    let p = rn(
        "f x = let x = 1; y = x in \\x -> case x of Just y -> y; z -> z\n",
        &[],
    )
    .unwrap();
    let out = main_of(&p);
    assert!(out.contains("(let ((fun x#2 () 1 ) (fun y#3 () x#2 )) (\\ (x#4) (case x#4 ((Prelude.Just y#5) -> y#5) (z#6 -> z#6))))"), "{out}");
}

#[test]
fn imports_qualified_hiding() {
    let a = "module A (T(..), val, hidden) where\ndata T = MkT Int | Other\nval = 1\nhidden = 2\n";
    let p = rn(
        "import qualified A as Q\nimport A hiding (hidden)\nf = (Q.val, val, Q.hidden, MkT 1)\n",
        &[("A", a)],
    )
    .unwrap();
    let out = main_of(&p);
    assert!(
        out.contains("(tuple A.val A.val A.hidden (A.MkT 1))"),
        "{out}"
    );
    let e = rn("import A hiding (hidden)\nf = hidden\n", &[("A", a)]).unwrap_err();
    assert!(e[0].starts_with("E0201"), "{e:?}");
    let e = rn("import A (nothing)\nf = 1\n", &[("A", a)]).unwrap_err();
    assert!(e[0].starts_with("E0205"), "{e:?}");
    // Ambiguity between two modules.
    let b = "module B where\nval = 3\n";
    let e = rn("import A\nimport B\nf = val\n", &[("A", a), ("B", b)]).unwrap_err();
    assert!(e[0].starts_with("E0202"), "{e:?}");
    // Re-export with `module M`.
    let c = "module C (module A) where\nimport A\n";
    let p = rn("import C\nf = val\n", &[("A", a), ("C", c)]).unwrap();
    assert!(main_of(&p).contains("A.val"));
}

#[test]
fn name_errors() {
    let e = rn("f = undefinedThing\n", &[]).unwrap_err();
    assert!(
        e[0].starts_with("E0201: name `undefinedThing` is not in scope"),
        "{e:?}"
    );
    let e = rn("f = 1\ng = 2\nf = 3\n", &[]).unwrap_err();
    assert!(e[0].starts_with("E0203"), "{e:?}");
    let e = rn("f (x, x) = x\n", &[]).unwrap_err();
    assert!(e[0].starts_with("E0203"), "{e:?}");
    let e = rn("g :: Int\nf = 1\n", &[]).unwrap_err();
    assert!(e[0].starts_with("E0606"), "{e:?}");
    let e = rn(
        "infix 4 ===\ninfix 4 /==\na === b = a\na /== b = b\nf = 1 === 2 /== 3\n",
        &[],
    )
    .unwrap_err();
    assert!(e[0].starts_with("E0204"), "{e:?}");
    let e = rn("import Missing\nf = 1\n", &[]).unwrap_err();
    assert!(e[0].contains("module `Missing` not found"), "{e:?}");
    let e = rn("{-# LANGUAGE TemplateHaskell #-}\nf = 1\n", &[]).unwrap_err();
    assert!(e[0].starts_with("E0001"), "{e:?}");
    let x = "module X where\nimport Y\n";
    let y = "module Y where\nimport X\n";
    let e = rn("import X\nf = 1\n", &[("X", x), ("Y", y)]).unwrap_err();
    assert!(e.iter().any(|m| m.contains("import cycle")), "{e:?}");
}

#[test]
fn display_names() {
    assert_eq!(display_name("x#12"), "x");
    assert_eq!(display_name("#"), "#");
    assert_eq!(display_name("a#b"), "a#b");
}
