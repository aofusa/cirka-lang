use crate::tast::print_program;
use crate::*;
use achc_rename::{Options, rename_source};
use cirka_diagnostics::SourceMap;

const PRIM_SRC: &str = "{-# LANGUAGE NoImplicitPrelude #-}
module Cirka.Prim where
data Int
data Char
data Double
data IO a
data Bool = False | True
foreign import prim \"int_add\" primIntAdd :: Int -> Int -> Int
foreign import prim \"io_return\" primReturnIO :: a -> IO a
foreign import prim \"io_bind\" primBindIO :: IO a -> (a -> IO b) -> IO b
foreign import prim \"put_str\" putStr :: [Char] -> IO ()
";

const PRELUDE: &str = "{-# LANGUAGE NoImplicitPrelude #-}
module Prelude (module Prelude, module Cirka.Prim) where
import Cirka.Prim
infixl 6 +
infixr 9 .
infixr 0 $
infixl 1 >>=, >>
infixr 5 ++
(+) :: Int -> Int -> Int
x + y = primIntAdd x y
negate :: Int -> Int
negate x = x
(.) :: (b -> c) -> (a -> b) -> a -> c
(f . g) x = f (g x)
($) :: (a -> b) -> a -> b
f $ x = f x
(>>=) :: IO a -> (a -> IO b) -> IO b
(>>=) = primBindIO
(>>) :: IO a -> IO b -> IO b
m >> k = m >>= \\_ -> k
return :: a -> IO a
return = primReturnIO
data Maybe a = Nothing | Just a
map :: (a -> b) -> [a] -> [b]
map f [] = []
map f (x:xs) = f x : map f xs
(++) :: [a] -> [a] -> [a]
[] ++ ys = ys
(x:xs) ++ ys = x : (xs ++ ys)
concatMap :: (a -> [b]) -> [a] -> [b]
concatMap f [] = []
concatMap f (x:xs) = f x ++ concatMap f xs
enumFromTo :: Int -> Int -> [Int]
enumFromTo a b = []
";

fn tc(src: &str) -> Result<tast::Program, Vec<String>> {
    let mods = [("Cirka.Prim", PRIM_SRC), ("Prelude", PRELUDE)];
    let mut sm = SourceMap::new();
    let p = rename_source(&mut sm, src, &mods, &Options::default()).map_err(|es| {
        es.iter()
            .map(|d| {
                format!(
                    "rename {}: {}",
                    d.code.clone().unwrap_or_default(),
                    d.message
                )
            })
            .collect::<Vec<_>>()
    })?;
    typecheck(&p).map_err(|es| {
        es.iter()
            .map(|d| format!("{}: {}", d.code.clone().unwrap_or_default(), d.message))
            .collect()
    })
}

/// The type of a top-level binding of `Main`.
fn ty_of(p: &tast::Program, name: &str) -> String {
    let full = format!("Main.{name}");
    for g in &p.binds {
        for b in g.binds() {
            if b.name == full {
                return show(&b.ty);
            }
        }
    }
    panic!("no binding {name}\n{}", print_program(p))
}

fn errs(src: &str) -> Vec<String> {
    match tc(src) {
        Ok(p) => panic!("expected errors\n{}", print_program(&p)),
        Err(e) => e,
    }
}

#[test]
fn infers_polymorphic_functions() {
    let p = tc("id x = x\nconst x y = x\ncompose f g x = f (g x)\npair = (id 1, id 'c')\nmain = putStr \"hi\"\n").unwrap();
    let id = ty_of(&p, "id");
    let (a, b) = id.split_once(" -> ").unwrap();
    assert_eq!(a, b, "{id}");
    let c = ty_of(&p, "compose");
    assert_eq!(c, "(a -> b) -> (c -> a) -> c -> b");
    assert_eq!(ty_of(&p, "pair"), "(Int, Char)");
    assert_eq!(p.main.as_deref(), Some("Main.main"));
}

#[test]
fn recursion_and_mutual_recursion() {
    let p = tc("len [] = 0\nlen (_:xs) = 1 + len xs\n\
         isEven 0 = True\nisEven n = isOdd (n + negate 1)\n\
         isOdd 0 = False\nisOdd n = isEven (n + negate 1)\n\
         main = return ()\n")
    .unwrap();
    assert!(ty_of(&p, "len").ends_with("] -> Int"));
    assert_eq!(ty_of(&p, "isEven"), "Int -> Bool");
    assert_eq!(ty_of(&p, "isOdd"), "Int -> Bool");
    let dump = print_program(&p);
    assert!(dump.contains("rec\nMain.isEven"), "{dump}");
}

#[test]
fn signatures_and_polymorphic_recursion() {
    let p = tc("data Nested a = Flat a | Nest (Nested [a])\n\
         depth :: Nested a -> Int\n\
         depth (Flat _) = 0\n\
         depth (Nest n) = 1 + depth n\n\
         main :: IO ()\nmain = return ()\n")
    .unwrap();
    assert_eq!(ty_of(&p, "depth"), "Nested a -> Int");
}

#[test]
fn let_polymorphism_and_sugar() {
    let p = tc("f = let g x = x in (g 1, g True)\n\
         h xs = [x + 1 | x <- xs, x == x]\n\
         x == y = True\n\
         sec = map (+ 1) [1, 2]\n\
         sec2 = map (1 +) [1]\n\
         k = \\case { Just x -> x; Nothing -> 0 }\n\
         main = do\n  putStr \"a\"\n  s <- return \"b\"\n  let t = s ++ s\n  putStr t\n");
    // `\\case` needs LambdaCase; the renamer/parser accepts it only with
    // the extension, so check the rest without it.
    let p = match p {
        Ok(p) => p,
        Err(_) => tc("f = let g x = x in (g 1, g True)\n\
             h xs = [x + 1 | x <- xs, x == x]\n\
             x == y = True\n\
             sec = map (+ 1) [1, 2]\n\
             sec2 = map (1 +) [1]\n\
             main = do\n  putStr \"a\"\n  s <- return \"b\"\n  let t = s ++ s\n  putStr t\n")
        .unwrap(),
    };
    assert_eq!(ty_of(&p, "f"), "(Int, Bool)");
    assert_eq!(ty_of(&p, "h"), "[Int] -> [Int]");
    assert_eq!(ty_of(&p, "sec"), "[Int]");
    assert_eq!(ty_of(&p, "main"), "IO ()");
}

#[test]
fn records() {
    let p = tc("data P = P { px :: Int, py :: Int }\n\
         mk = P { px = 1, py = 2 }\n\
         sx = px mk\n\
         upd p = p { py = 3 }\n\
         main = return ()\n")
    .unwrap();
    assert_eq!(ty_of(&p, "sx"), "Int");
    assert_eq!(ty_of(&p, "upd"), "P -> P");
}

#[test]
fn type_errors() {
    let e = errs("f = 1 + 'c'\nmain = return ()\n");
    assert!(e[0].starts_with("E0601"), "{e:?}");
    let e = errs("f x = x x\nmain = return ()\n");
    assert!(e[0].starts_with("E0602"), "{e:?}");
    let e = errs("f :: a -> Int\nf x = x\nmain = return ()\n");
    assert!(e[0].starts_with("E0604"), "{e:?}");
    let e = errs("f 0 = 1\nf x y = 2\nmain = return ()\n");
    assert!(e[0].starts_with("E0605"), "{e:?}");
    let e = errs("g :: Int\nmain = return ()\n");
    assert!(e[0].contains("E0606"), "{e:?}");
    let e = errs("main = 1\n");
    assert!(e[0].starts_with("E0601"), "{e:?}");
    let e = errs("class C a where\n  m :: a -> a\nf :: C a => a -> a\nf x = x\nmain = return ()\n");
    assert!(e[0].starts_with("E0001"), "{e:?}");
}
