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
foreign import prim \"int_sub\" primIntSub :: Int -> Int -> Int
foreign import prim \"int_eq\" primIntEq :: Int -> Int -> Bool
foreign import prim \"show_int\" primShowInt :: Int -> [Char]
foreign import prim \"io_return\" primReturnIO :: a -> IO a
foreign import prim \"io_bind\" primBindIO :: IO a -> (a -> IO b) -> IO b
foreign import prim \"put_str\" putStr :: [Char] -> IO ()
foreign import prim \"error\" error :: [Char] -> a
";

const PRELUDE: &str = "{-# LANGUAGE NoImplicitPrelude #-}
module Prelude (module Prelude, module Cirka.Prim) where
import Cirka.Prim
infixl 6 +, -
infix 4 ==
infixr 5 ++
infixl 1 >>=, >>
infixr 0 $
(+), (-) :: Int -> Int -> Int
(+) = primIntAdd
(-) = primIntSub
(==) :: Int -> Int -> Bool
(==) = primIntEq
negate :: Int -> Int
negate x = 0 - x
($) :: (a -> b) -> a -> b
f $ x = f x
(>>=) :: IO a -> (a -> IO b) -> IO b
(>>=) = primBindIO
(>>) :: IO a -> IO b -> IO b
m >> k = m >>= \\_ -> k
return :: a -> IO a
return = primReturnIO
otherwise :: Bool
otherwise = True
show :: Int -> [Char]
show = primShowInt
(++) :: [a] -> [a] -> [a]
[] ++ ys = ys
(x:xs) ++ ys = x : (xs ++ ys)
concatMap :: (a -> [b]) -> [a] -> [b]
concatMap _ [] = []
concatMap f (x:xs) = f x ++ concatMap f xs
enumFromTo :: Int -> Int -> [Int]
enumFromTo a b = if a == b then [b] else a : enumFromTo (a + 1) b
print :: Int -> IO ()
print n = putStr (show n ++ \"\\n\")
";

fn compile(src: &str) -> Result<Output, Vec<String>> {
    let mods = [("Cirka.Prim", PRIM_SRC), ("Prelude", PRELUDE)];
    let mut sm = SourceMap::new();
    let fmt = |es: Vec<Diagnostic>| -> Vec<String> {
        es.iter()
            .map(|d| format!("{}: {}", d.code.clone().unwrap_or_default(), d.message))
            .collect()
    };
    let p = rename_source(&mut sm, src, &mods, &Options::default()).map_err(fmt)?;
    let t = achc_typeck::typecheck(&p).map_err(fmt)?;
    desugar(&t).map_err(fmt)
}

/// Runs the program in the MIR interpreter (optimized and not).
fn run(src: &str) -> String {
    let out = compile(src).unwrap_or_else(|e| panic!("{e:?}"));
    let mut results = Vec::new();
    for opt in [false, true] {
        let mut p = out.program.clone();
        if opt {
            c::opt::optimize(&mut p).unwrap_or_else(|e| panic!("{}", e.message));
        }
        let mut m = cirka_mir::lower::lower_program(&p)
            .unwrap_or_else(|e| panic!("{:?}\n{}", e, c::print_program(&p)));
        cirka_mir::perceus::run(&mut m);
        let errs = cirka_mir::verify::verify_module(&m);
        assert!(errs.is_empty(), "{errs:?}");
        let r = cirka_mir::interp::run(&m, b"", 10_000_000).unwrap();
        // `error` does not unwind: only a normal exit must free everything.
        if r.exit_code == 0 {
            assert_eq!(r.leaked, 0, "leak (opt={opt})");
        }
        results.push(format!(
            "{}{}",
            r.stdout,
            if r.exit_code != 0 {
                format!("[exit {}]", r.exit_code)
            } else {
                String::new()
            }
        ));
    }
    assert_eq!(results[0], results[1]);
    results.pop().unwrap()
}

fn warnings(src: &str) -> Vec<String> {
    compile(src)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .warnings
        .iter()
        .map(|d| d.code.clone().unwrap_or_default())
        .collect()
}

#[test]
fn hello_and_arithmetic() {
    assert_eq!(run("main = putStr \"hello, wörld\\n\"\n"), "hello, wörld\n");
    assert_eq!(
        run("fact 0 = 1\nfact n = n + fact (n - 1)\nmain = print (fact 10)\n"),
        "56\n"
    );
}

#[test]
fn lists_and_laziness() {
    let src = "nats n = n : nats (n + 1)\n\
        take 0 _ = []\n\
        take _ [] = []\n\
        take n (x:xs) = x : take (n - 1) xs\n\
        sum [] = 0\n\
        sum (x:xs) = x + sum xs\n\
        main = do\n  print (sum (take 5 (nats 1)))\n  print (sum [x + x | x <- [1 .. 4]])\n";
    assert_eq!(run(src), "15\n20\n");
}

#[test]
fn guards_where_and_fallthrough() {
    let src = "classify n\n  | n == 0 = \"zero\"\n  | small = \"small\"\n  | otherwise = \"big\"\n  where small = n == 1\n\
        f (Just 0) = 0\nf (Just n) | n == 5 = 50\nf _ = 1\n\
        data Maybe a = Nothing | Just a\n\
        main = do\n  putStr (classify 0 ++ classify 1 ++ classify 7 ++ \"\\n\")\n  print (f (Just 0) + f (Just 5) + f (Just 2) + f Nothing)\n";
    assert_eq!(run(src), "zerosmallbig\n52\n");
}

#[test]
fn tuples_chars_and_strings() {
    let src = "swap (a, b) = (b, a)\n\
        isA 'a' = True\nisA _ = False\n\
        greet \"bob\" = \"hi bob\"\ngreet _ = \"who?\"\n\
        count [] = 0\ncount (c:cs) = if isA c then 1 + count cs else count cs\n\
        main = do\n  let (x, y) = swap (1, 2)\n  print (x - y)\n  print (count \"banana\")\n  putStr (greet \"bob\" ++ greet \"al\")\n";
    assert_eq!(run(src), "1\n3\nhi bobwho?");
}

#[test]
fn lazy_patterns_and_errors() {
    let src = "f ~(a, b) = 1\nmain = do\n  print (f undefined)\n  print (g 2)\n\
        undefined = error \"boom\"\ng 1 = 1\n";
    let out = compile(src).unwrap();
    assert!(
        out.warnings
            .iter()
            .any(|w| w.code.as_deref() == Some("W0610"))
    );
    assert_eq!(run(src), "1\n[exit 1]");
}

#[test]
fn exhaustiveness_warnings() {
    assert_eq!(
        warnings("f True = 1\nmain = print (f True)\n"),
        vec!["W0610"]
    );
    assert_eq!(
        warnings("f True = 1\nf False = 2\nmain = print (f True)\n"),
        Vec::<String>::new()
    );
    assert_eq!(
        warnings("f _ = 1\nf True = 2\nmain = print (f True)\n"),
        vec!["W0611"]
    );
    assert_eq!(
        warnings("f n | n == 0 = 1\n    | otherwise = 2\nmain = print (f 0)\n"),
        Vec::<String>::new()
    );
}
