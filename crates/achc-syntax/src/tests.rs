use crate::lexer::{Tok, lex};
use crate::*;
use cirka_diagnostics::FileId;

fn toks(src: &str) -> Vec<Tok> {
    let l = lex(FileId(0), src);
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    l.tokens.into_iter().map(|t| t.tok).collect()
}

fn ok(src: &str) -> String {
    let (m, errs) = parse(FileId(0), src);
    assert!(
        errs.is_empty(),
        "unexpected errors: {:?}",
        errs.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    print::print_module(&m)
}

fn err(src: &str) -> Vec<String> {
    let (_, errs) = parse(FileId(0), src);
    assert!(!errs.is_empty(), "expected a syntax error");
    errs.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn lexes_names_and_literals() {
    use Tok::*;
    assert_eq!(
        toks("M.x M.N.T M.+ x.y Foo.where 0x1F 1.5e3 'a' '\\n' \"a\\&b\\x41\""),
        vec![
            VarId(Some("M".into()), "x".into()),
            ConId(Some("M.N".into()), "T".into()),
            VarSym(Some("M".into()), "+".into()),
            VarId(None, "x".into()),
            VarSym(None, ".".into()),
            VarId(None, "y".into()),
            ConId(None, "Foo".into()),
            VarSym(None, ".".into()),
            Kw(crate::lexer::Kw::Where),
            Int(31),
            Float(1500.0),
            Char('a'),
            Char('\n'),
            Str("abA".into()),
            Eof
        ]
    );
    // `-->` is an operator, `--` starts a comment; `{- -}` nests.
    assert_eq!(
        toks("a --> b -- c\n{- x {- y -} z -} d"),
        vec![
            VarId(None, "a".into()),
            VarSym(None, "-->".into()),
            VarId(None, "b".into()),
            VarId(None, "d".into()),
            Eof
        ]
    );
    assert_eq!(
        toks("[1..10] \\SOH"),
        vec![
            LBracket,
            Int(1),
            DotDot,
            Int(10),
            RBracket,
            Backslash,
            ConId(None, "SOH".into()),
            Eof
        ]
    );
    assert_eq!(
        toks("'\\SOH' '\\SO' '\\^A' '\\DEL'")[..4],
        [Char('\x01'), Char('\x0e'), Char('\x01'), Char('\x7f')]
    );
}

#[test]
fn language_pragmas() {
    let l = lex(
        FileId(0),
        "{-# LANGUAGE BangPatterns, LambdaCase #-}\n{-# OPTIONS_GHC -O2 #-}\nmodule M where\n",
    );
    let names: Vec<&str> = l.extensions.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(names, ["BangPatterns", "LambdaCase"]);
}

#[test]
fn lexical_errors() {
    let l = lex(FileId(0), "x = \"abc\ny = 'ab'\nz = {- open");
    assert_eq!(l.errors.len(), 3, "{:?}", l.errors);
}

/// The layout example of Report §10.3.
#[test]
fn report_layout_example() {
    let out = ok(r#"module AStack( Stack, push, pop, top, size ) where
data Stack a = Empty
             | MkStack a (Stack a)

push :: a -> Stack a -> Stack a
push x s = MkStack x s

size :: Stack a -> Int
size s = length (stkToLst s)  where
           stkToLst  Empty         = []
           stkToLst (MkStack x s)  = x:xs where xs = stkToLst s

pop :: Stack a -> (a, Stack a)
pop (MkStack x s)
  = (x, case s of r -> i r where i x = x) -- (pop Empty) is an error

top :: Stack a -> a
top (MkStack x s) = x                     -- (top Empty) is an error
"#);
    let expected = r#"(module AStack
  (exports Stack push pop top size)
  (data Stack (a)
    (con Empty )
    (con MkStack a (Stack a))
  )
  (sig (push) (-> a (-> (Stack a) (Stack a))))
  (fun push (x s)
    ((MkStack x) s)
  )
  (sig (size) (-> (Stack a) Int))
  (fun size (s)
    (length (paren (stkToLst s)))
    (where
      (fun stkToLst (Empty)
        []
      )
      (fun stkToLst ((MkStack x s))
        (ops x `:` xs)
        (where
          (fun xs ()
            (stkToLst s)
          )
        )
      )
    )
  )
  (sig (pop) (-> (Stack a) (tuple a (Stack a))))
  (fun pop ((MkStack x s))
    (tuple x (case s (r -> (i r) (where (fun i (x) x ) ))))
  )
  (sig (top) (-> (Stack a) a))
  (fun top ((MkStack x s))
    x
  )
)
"#;
    assert_eq!(out, expected);
}

#[test]
fn parse_error_rule_closes_blocks() {
    // `let` on one line: `in` closes the implicit block.
    let out =
        ok("f = let x = 1; y = 2 in x\ng = (case 1 of 1 -> 2)\nh = [x | let y = 1, x <- [y]]\n");
    assert!(
        out.contains("(let ((fun x () 1 ) (fun y () 2 )) x)"),
        "{out}"
    );
    assert!(out.contains("(paren (case 1 (1 -> 2)))"), "{out}");
    assert!(
        out.contains("(listcomp x (let (fun y () 1 )) (<- x [y]))"),
        "{out}"
    );
    // `do` inside parentheses and a trailing `where`.
    let out = ok("main = print (do x <- [1]; return x)\n  where y = 1\n");
    assert!(out.contains("(do (<- x [1]) (return x))"), "{out}");
    assert!(out.contains("(where"), "{out}");
}

#[test]
fn nested_layout_and_do() {
    let out = ok(r#"main :: IO ()
main = do
  let xs = [1, 2, 3]
      n = length xs
  if n > 2
    then putStrLn "big"
    else return ()
  mapM_ print xs
  case xs of
    [] -> return ()
    (y:_) | y > 0 -> print y
          | otherwise -> return ()
"#);
    assert!(
        out.contains("(let (fun xs () [1 2 3] ) (fun n () (length xs) ))"),
        "{out}"
    );
    assert!(
        out.contains("(if (ops n `>` 2) (putStrLn \"big\") (return ()))"),
        "{out}"
    );
    assert!(
        out.contains(
            "((ops y `:` _) -> (| ((ops y `>` 0)) (print y)) (| (otherwise) (return ())))"
        ),
        "{out}"
    );
}

#[test]
fn do_and_if_then_else() {
    let out = ok("main = do\n  if True\n  then return ()\n  else return ()\n");
    assert!(
        out.contains("(do (if True (return ()) (return ())))"),
        "{out}"
    );
}

#[test]
fn expressions() {
    let (e, errs) = parse_expr(
        FileId(0),
        "(\\x y -> x + y * 2) (- 3) `div` f $ (+ 1) . (`elem` xs) . (2 ^) . (,) 1 . (, True)",
    );
    assert!(errs.is_empty(), "{errs:?}");
    let s = print::expr(&e.unwrap());
    assert_eq!(
        s,
        "(ops ((paren (\\ (x y) (ops x `+` y `*` 2))) (paren (ops neg 3))) `div` f `$` (section `+` 1) `.` (section `elem` xs) `.` (section 2 `^`) `.` ((,) 1) `.` (tuple-section _ True))"
    );
    let (e, _) = parse_expr(
        FileId(0),
        "[x * y | x <- [1..3], odd x, y <- [x, x+2 .. 9]]",
    );
    assert_eq!(
        print::expr(&e.unwrap()),
        "(listcomp (ops x `*` y) (<- x (range 1 _ 3)) (odd x) (<- y (range x (ops x `+` 2) 9)))"
    );
    let (e, _) = parse_expr(FileId(0), "P { px = 1, py } { px = 2 } :: P");
    assert_eq!(
        print::expr(&e.unwrap()),
        "(:: (update (record P (px 1) py) (px 2)) P)"
    );
}

#[test]
fn declarations() {
    let out = ok(r#"module M (T(..), (+++), module N, f) where
import qualified Data.List as L hiding (sort)
import Prelude (Maybe(..), (++), map)
infixr 5 +++, `cons`
data T a = Leaf | Node !(T a) a (T a) | a :+: a deriving (Eq, Show)
newtype W = W { unW :: Int }
type S = [Char]
class Eq a => C a where
  op :: a -> a
  op = id
instance C Int where op x = x + 1
(+++) :: [a] -> [a] -> [a]
xs +++ ys = foldr (:) ys xs
f :: (Ord a, Show a) => a -> String
f !x | x > x = "no"
     | otherwise = show x
(a, b) = (1, 2)
Just z = Nothing
g n@(Just _) ~(x:_) (-1) = 0
foreign import ccall unsafe "math.h sin" c_sin :: Double -> Double
"#);
    let expected = r#"(module M
  (exports T(..) +++ (module N) f)
  (import Data.List qualified as L hiding (sort))
  (import Prelude (Maybe(..) ++ map))
  (infixr 5 +++ cons)
  (data T (a)
    (con Leaf )
    (con Node !(T a) a (T a))
    (con :+: a a)
    (deriving Eq Show)
  )
  (newtype W ()
    (con W (record (unW Int)))
  )
  (type S () [Char])
  (class ((Eq a)) C (a)
    (sig (op) (-> a a))
    (fun op ()
      id
    )
  )
  (instance () C Int
    (fun op (x)
      (ops x `+` 1)
    )
  )
  (sig (+++) (-> [a] (-> [a] [a])))
  (fun +++ (xs ys)
    (((foldr :) ys) xs)
  )
  (sig (f) (=> ((Ord a) (Show a)) (-> a String)))
  (fun f (!x)
    (| ((ops x `>` x)) "no")
    (| (otherwise) (show x))
  )
  (patbind (tuple a b)
    (tuple 1 2)
  )
  (patbind (Just z)
    Nothing
  )
  (fun g (n@(Just _) ~(ops x `:` _) -1)
    0
  )
  (foreign import ccall "math.h sin" c_sin (-> Double Double))
)
"#;
    assert_eq!(out, expected);
}

#[test]
fn syntax_errors_do_not_panic() {
    let msgs = err("f x = if x then 1\n");
    assert!(
        msgs.iter().any(|m| m.contains("expected keyword `else`")),
        "{msgs:?}"
    );
    err("data = 1\n");
    err("f = (1, \n");
    err("main = do\n");
    err("x :: \n");
    // Arbitrary garbage must not panic or loop.
    for src in [
        ")))",
        "(((",
        "let in",
        "where where",
        "f = \\ ->",
        "case of",
        "{ ; }",
        "import",
        "module",
        "data T = | |",
        "f = [1 ..",
        "x = y {",
        "instance",
        "infixl 99 +",
        "`",
        "f = \\case\n",
        "f = if | x -> 1\n",
        "x = 'a",
    ] {
        let _ = parse(FileId(0), src);
    }
}
