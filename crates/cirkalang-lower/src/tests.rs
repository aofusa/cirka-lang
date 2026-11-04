use crate::*;
use cirka_diagnostics::SourceMap;
use cirkalang_resolve::{MemLoader, Options, resolve_program};

const PRELUDE: &str = include_str!("../../../lib/cirkalang/core/Prelude.cirka");

fn lower_src(src: &str) -> c::Program {
    let mut files = HashMap::new();
    files.insert("Prelude".to_string(), PRELUDE.to_string());
    let mut sm = SourceMap::new();
    let id = sm.add("Main.cirka", src.to_string());
    let rp = resolve_program(&mut sm, id, src, &mut MemLoader(files), &Options::default())
        .unwrap_or_else(|e| {
            panic!(
                "resolve: {:?}",
                e.iter().map(|d| &d.message).collect::<Vec<_>>()
            )
        });
    let el = cirkalang_elab::elaborate(&rp);
    let p = el.program.unwrap_or_else(|| {
        panic!(
            "elab: {:?}",
            el.diags
                .iter()
                .map(|d| format!("{:?} {}", d.code, d.message))
                .collect::<Vec<_>>()
        )
    });
    lower(&p).unwrap_or_else(|e| {
        panic!(
            "lower: {:?}",
            e.iter().map(|d| &d.message).collect::<Vec<_>>()
        )
    })
}

const VECT: &str = r#"
data Vect : (0 n : Nat) -> (a : Type) -> Type where
  VNil : Vect Z a
  VCons : a -> Vect n a -> Vect (S n) a

vappend : Vect n a -> Vect m a -> Vect (plus n m) a
vappend VNil ys = ys
vappend (VCons x xs) ys = VCons x (vappend xs ys)

vlength : Vect n a -> Int
vlength VNil = 0
vlength (VCons _ xs) = 1 + vlength xs

replicate : (n : Nat) -> a -> Vect n a
replicate Z _ = VNil
replicate (S k) x = VCons x (replicate k x)

main : IO ()
main = do
  let v = vappend (replicate 2 7) (VCons 1 VNil)
  printInt (vlength v)
"#;

#[test]
fn erases_indices_and_types() {
    let p = lower_src(VECT);
    // The index `n` of Vect is gone; the element type became `Any`.
    let vect = p.data("Main.Vect").unwrap();
    assert!(vect.params.is_empty());
    let out = c::print_program(&p);
    assert!(out.contains("(Main.VCons (! Any) (! Main.Vect))"), "{out}");
    // `replicate` keeps its runtime Nat argument but no type arguments.
    let r = p.bind("Main.replicate").unwrap();
    let rb = c::print::print_bind(r);
    assert!(rb.starts_with("(def Main.replicate (forall (a"), "{rb}");
    assert!(
        rb.lines().next().unwrap().contains("(-> Prelude.Nat a"),
        "{rb}"
    );
    let a = p.bind("Main.vappend").unwrap();
    let ab = c::print::print_bind(a);
    // The Nat indices n and m are erased; only the element type remains.
    let first = ab.lines().next().unwrap();
    assert!(
        first.starts_with("(def Main.vappend (forall (a")
            && first.contains("(-> Main.Vect Main.Vect Main.Vect)")
            && !first.contains("Nat"),
        "{ab}"
    );
}

#[test]
fn polymorphism_becomes_forall() {
    let p = lower_src("main : IO ()\nmain = printInt (length (map (\\x => x + 1) [1, 2, 3]))\n");
    let m = p.bind("Prelude.map").unwrap();
    let t = c::print::print_bind(m);
    assert!(t.starts_with("(def Prelude.map (forall (a"), "{t}");
    assert!(p.bind("main").is_some());
}

#[test]
fn prelude_lowers_and_lints() {
    let p = lower_src(
        r#"
fact : Int -> Int
fact 0 = 1
fact n = n * fact (n - 1)

greet : String -> String
greet "world" = "hello!"
greet s = "hi " ++ s

main : IO ()
main = do
  printInt (fact 10)
  putStrLn (greet "world")
  printFloat (sqrt 2.0)
  putStrLn (showInt (-42) ++ showChar 'x')
"#,
    );
    assert!(p.bind("main").is_some());
}
