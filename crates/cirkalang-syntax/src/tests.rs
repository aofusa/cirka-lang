use crate::lexer::{Tok, lex};
use crate::*;
use cirka_diagnostics::FileId;

fn toks(src: &str) -> Vec<Tok> {
    let (t, e) = lex(FileId(0), src);
    assert!(e.is_empty(), "{e:?}");
    t.into_iter().map(|t| t.tok).collect()
}

fn layout_str(src: &str) -> String {
    let (t, _) = lex(FileId(0), src);
    layout::layout(t)
        .into_iter()
        .map(|t| match t.tok {
            Tok::VLBrace => "{".to_string(),
            Tok::VSemi => ";".to_string(),
            Tok::VRBrace => "}".to_string(),
            Tok::Eof => String::new(),
            Tok::Kw(k) => k.name().to_string(),
            Tok::Int(n) => n.to_string(),
            other => other.describe().trim_matches('`').to_string(),
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_ok(src: &str) -> String {
    let (m, errs) = parse(FileId(0), src);
    assert!(
        errs.is_empty(),
        "{:?}",
        errs.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    print::print_module(&m)
}

#[test]
fn lexes_literals_and_names() {
    assert_eq!(
        toks("x A.B.y A.B 0xFF 1_000 1.5e3 'a' '\\n' \"s\\t\" ?h _ :: -> => <- ;"),
        vec![
            Tok::LIdent(vec![], "x".into()),
            Tok::LIdent(vec!["A".into(), "B".into()], "y".into()),
            Tok::UIdent(vec!["A".into()], "B".into()),
            Tok::Int(255),
            Tok::Int(1000),
            Tok::Float(1500.0),
            Tok::Char('a'),
            Tok::Char('\n'),
            Tok::Str("s\t".into()),
            Tok::Hole("h".into()),
            Tok::Underscore,
            Tok::Op(vec![], "::".into()),
            Tok::Arrow,
            Tok::FatArrow,
            Tok::LArrow,
            Tok::Semi,
            Tok::Eof,
        ]
    );
}

#[test]
fn comments_and_dash_operators() {
    assert_eq!(
        toks("a -- comment\n{- block {- nested -} -} b --> c"),
        vec![
            Tok::LIdent(vec![], "a".into()),
            Tok::LIdent(vec![], "b".into()),
            Tok::Op(vec![], "-->".into()),
            Tok::LIdent(vec![], "c".into()),
            Tok::Eof
        ]
    );
}

#[test]
fn lexical_errors_are_reported() {
    for src in ["'ab'", "\"abc", "{- open", "'\\q'", "1x", "0x", "€"] {
        let (_, e) = lex(FileId(0), src);
        assert!(!e.is_empty(), "no error for {src:?}");
    }
}

#[test]
fn layout_inserts_braces() {
    assert_eq!(
        layout_str("f = do\n  a\n  b\ng = 1"),
        "; f = do { a ; b } ; g = 1 }"
    );
    assert_eq!(
        layout_str("x = let a = 1\n        b = 2\n    in a"),
        "; x = let { a = 1 ; b = 2 } in a }"
    );
    assert_eq!(
        layout_str("x = (case y of A => 1, 2)"),
        "; x = ( case y of { A => 1 } , 2 ) }"
    );
    assert_eq!(
        layout_str("x = let a = 1 in a"),
        "; x = let { a = 1 } in a }"
    );
}

#[test]
fn parses_declarations() {
    let src = r#"
module Main
import Core.List

infixl 6 +, -
data List (a : Type) where
  Nil  : List a
  (::) : a -> List a -> List a
data Maybe a = Nothing | Just a
data Vect : (0 n : Nat) -> (a : Type) -> Type where
  VNil  : Vect 0 a
  VCons : a -> Vect n a -> Vect (S n) a

map : (a -> b) -> List a -> List b
map f [] = []
map f (x :: xs) = f x :: map f xs

x ++ y = append x y
private swap : (1 p : (a, b)) -> (b, a)
swap (x, y) = (y, x)
"#;
    let out = parse_ok(src);
    let expected = "(module Main)
(import Core.List)
(infixl 6 + -)
(data List (a : Type)
  (Nil : (List a))
  (:: : (-> a (-> (List a) (List a)))))
(data Maybe a
  (Nothing )
  (Just a))
(data Vect : (-> (0 n : Nat) (-> (a : Type) Type))
  (VNil : (Vect 0 a))
  (VCons : (-> a (-> (Vect n a) (Vect (S n) a)))))
(sig (map) (-> (-> a b) (-> (List a) (List b))))
(def map (f []) [])
(def map (f (ops x :: xs)) (ops (f x) :: (map f xs)))
(def ++ (x y) (append x y))
(private sig (swap) (-> (1 p : (tuple a b)) (tuple b a)))
(def swap ((tuple x y)) (tuple y x))
";
    assert_eq!(out, expected);
}

#[test]
fn parses_expressions() {
    let src = r#"
main : IO ()
main = do
  x <- readInt
  let y = x + 1
      z = - y * 2
  if y > 0
    then printInt y
    else printInt (negate y)
  case z of
    0 => pure ()
    n => printInt n
  pure ()
f = \x, (1 y : Int) => (x : Int)
g = map (+ 1) . filter (`elem` xs) $ ys
h = {a : Type} -> (x : a) -> a
k = (x, y, ?hole, _)
"#;
    let out = parse_ok(src);
    let expected = "(sig (main) (IO ()))
(def main () (do (x <- readInt) (let (y = (ops x + 1)) (z = (ops neg y * 2))) (if (ops y > 0) (printInt y) (printInt (negate y))) (case z (0 => (pure ())) (n => (printInt n))) (pure ())))
(def f () (\\ (x (1 y : Int)) (the Int x)))
(def g () (ops (map (section + 1)) . (filter (section `elem` xs)) $ ys))
(def h () (-> {a : Type} (-> (x : a) a)))
(def k () (tuple x y ?hole _))
";
    assert_eq!(out, expected);
}

#[test]
fn syntax_errors_recover_per_declaration() {
    let src = "f = (1 +\ng : Int\ng = 2\nh = )\nk = 3\n";
    let (m, errs) = parse(FileId(0), src);
    assert_eq!(
        errs.len(),
        2,
        "{:?}",
        errs.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    let out = print::print_module(&m);
    assert!(
        out.contains("(sig (g) Int)")
            && out.contains("(def g () 2)")
            && out.contains("(def k () 3)"),
        "{out}"
    );
}

/// Deterministic fuzzing: truncations and token deletions of valid input
/// never make the lexer or parser panic.
#[test]
fn parser_never_panics() {
    let src = "module M\nimport A\ndata T a = A a | B\nf : (1 x : T Int) -> {n : Nat} -> Int\nf (A x) = case x of\n  0 => let y = 1 in y\n  _ => do { a <- b; pure (a, [1, 2]) }\ng = \\x => if x then (+ 1) else (`div` 2)\n";
    let chars: Vec<char> = src.chars().collect();
    let mut seed: u64 = 12345;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in 0..chars.len() {
        let s: String = chars[..i].iter().collect();
        let _ = parse(FileId(0), &s);
    }
    let pieces = [
        "(", ")", "[", "]", "{", "}", "=>", "->", "\\", "let", "in", "case", "of", "do", "if",
        "then", "else", ",", ";", "::", "-", "\n ", "\n", "data", "where", "`", "_", "?", ":",
    ];
    for _ in 0..3000 {
        let mut s = chars.clone();
        for _ in 0..(next() % 4 + 1) {
            let at = (next() as usize) % (s.len() + 1);
            if next() % 2 == 0 && at < s.len() {
                s.remove(at);
            } else {
                let p = pieces[(next() as usize) % pieces.len()];
                for (j, c) in p.chars().enumerate() {
                    s.insert(at + j, c);
                }
            }
        }
        let s: String = s.into_iter().collect();
        let _ = parse(FileId(0), &s);
        let _ = parse_expr(FileId(0), &s);
    }
}
