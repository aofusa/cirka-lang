use crate::*;
use std::collections::HashMap;

fn run(files: &[(&str, &str)], root: &str) -> Result<(Program, SourceMap), String> {
    let mut map: HashMap<String, String> = files
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect();
    let text = map.remove(root).unwrap();
    let mut sm = SourceMap::new();
    let id = sm.add(format!("{root}.cirka"), text.clone());
    let mut loader = MemLoader(map);
    match resolve_program(&mut sm, id, &text, &mut loader, &Options::default()) {
        Ok(p) => Ok((p, sm)),
        Err(errs) => Err(errs
            .iter()
            .map(|e| format!("{}: {}", e.code.clone().unwrap_or_default(), e.message))
            .collect::<Vec<_>>()
            .join("\n")),
    }
}

const PRELUDE: &str = "
infixl 6 +, -
infixl 7 *
infixr 5 ::
infix 4 ==
prim Int : Type
prim primAdd : Int -> Int -> Int
negate : Int -> Int
negate x = x
(+) : Int -> Int -> Int
x + y = primAdd x y
(-) : Int -> Int -> Int
x - y = x
(*) : Int -> Int -> Int
x * y = x
(==) : Int -> Int -> Int
x == y = x
data List a = Nil | a :: List a
private secret : Int
secret = 1
";

#[test]
fn resolves_operators_by_fixity() {
    let (p, _) = run(
        &[
            ("Prelude", PRELUDE),
            ("Main", "f : Int -> Int\nf x = - x * 2 + 3 == x\ng : List Int\ng = 1 :: 2 :: Nil\nh : Int -> Int\nh = (+ 1)\n"),
        ],
        "Main",
    )
    .unwrap();
    let out = print::print_module(&p, "Main");
    assert_eq!(
        out,
        "(def Main.f : (-> Prelude.Int Prelude.Int)
    (x) = (Prelude.== (Prelude.+ (Prelude.negate (Prelude.* 'x 2)) 3) 'x))
(def Main.g : (Prelude.List Prelude.Int)
    () = (Prelude.:: 1 (Prelude.:: 2 Prelude.Nil)))
(def Main.h : (-> Prelude.Int Prelude.Int)
    () = (\\ (sec$1) (Prelude.+ 'sec$1 1)))
"
    );
}

#[test]
fn auto_binds_implicits_and_scopes_locals() {
    let (p, _) = run(
        &[
            ("Prelude", PRELUDE),
            ("Main", "map : (a -> b) -> List a -> List b\nmap f Nil = Nil\nmap f (x :: xs) = let y = f x\n                      in y :: map f xs\n"),
        ],
        "Main",
    )
    .unwrap();
    let out = print::print_module(&p, "Main");
    assert!(out.starts_with("(def Main.map : (-> {0 a : _} (-> {0 b : _} (-> (-> 'a 'b) (-> (Prelude.List 'a) (Prelude.List 'b)))))"), "{out}");
    assert!(
        out.contains(
            "((Prelude.:: x xs) 'f) = (let ((y = ('f 'x))) (Prelude.:: 'y (Main.map 'f 'xs)))"
        ) || out.contains(
            "(f (Prelude.:: x xs)) = (let ((y = ('f 'x))) (Prelude.:: 'y (Main.map 'f 'xs))))"
        ),
        "{out}"
    );
}

#[test]
fn reports_name_errors() {
    let e = run(
        &[
            ("Prelude", PRELUDE),
            ("Main", "f : Int\nf = y\ng : Int\ng = Prelude.secret\n"),
        ],
        "Main",
    )
    .unwrap_err();
    assert!(e.contains("E1101: `y` is not defined"), "{e}");
    assert!(e.contains("E1106: `Prelude.secret` is private"), "{e}");
    let e = run(
        &[
            ("Prelude", PRELUDE),
            ("Main", "f : Int\nf = 1\nf : Int\ng = 2\nh : Int\n"),
        ],
        "Main",
    )
    .unwrap_err();
    assert!(
        e.contains("E1103")
            && e.contains("E1107: `g` has no type signature")
            && e.contains("E1107: `h` has a signature but no definition"),
        "{e}"
    );
    let e = run(
        &[
            ("Prelude", PRELUDE),
            ("Main", "import A\nf : Int\nf = 1\n"),
            ("A", "import Main\n"),
        ],
        "Main",
    )
    .unwrap_err();
    assert!(e.contains("E1105: import cycle"), "{e}");
    let e = run(&[("Prelude", PRELUDE), ("Main", "import Nope\n")], "Main").unwrap_err();
    assert!(e.contains("E1104: module `Nope` not found"), "{e}");
    let e = run(
        &[
            ("Prelude", PRELUDE),
            ("Main", "f : Int -> Int\nf x = x == x == x\n"),
        ],
        "Main",
    )
    .unwrap_err();
    assert!(e.contains("E1111"), "{e}");
}

#[test]
fn ambiguity_and_qualified_names() {
    let files = [
        ("Prelude", PRELUDE),
        ("A", "x : Int\nx = 1\n"),
        ("B", "x : Int\nx = 2\n"),
        (
            "Main",
            "import A\nimport B\nf : Int\nf = x\ng : Int\ng = A.x + B.x\n",
        ),
    ];
    let e = run(&files, "Main").unwrap_err();
    assert!(e.contains("E1102: `x` is ambiguous"), "{e}");
    let ok = [
        ("Prelude", PRELUDE),
        ("A", "x : Int\nx = 1\n"),
        ("B", "x : Int\nx = 2\n"),
        (
            "Main",
            "import A\nimport B\nx : Int\nx = 3\ng : Int\ng = A.x + B.x + x\n",
        ),
    ];
    let (p, _) = run(&ok, "Main").unwrap();
    let out = print::print_module(&p, "Main");
    assert!(
        out.contains("(Prelude.+ (Prelude.+ A.x B.x) Main.x)"),
        "{out}"
    );
    // Dependency order: Prelude, A, B, Main.
    let names: Vec<&str> = p.modules.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Prelude", "A", "B", "Main"]);
}
