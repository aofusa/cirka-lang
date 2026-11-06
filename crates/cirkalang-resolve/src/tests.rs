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

const TEST_PRELUDE: &str = "
prim Int : Type
prim String : Type
prim Bool : Type
prim IO : Type -> Type
prim argCount : IO Int
prim getArg : Int -> IO String
prim strEq : String -> String -> Bool
prim putStrLn : String -> IO ()
prim pure : a -> IO a
prim exit : Int -> IO a
prim (==) : Int -> Int -> Bool
";

fn run_tests(files: &[(&str, &str)], mode: TestMode) -> Result<Program, String> {
    let mut map: HashMap<String, String> = files
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect();
    let text = map.remove("Main").unwrap();
    let mut sm = SourceMap::new();
    let id = sm.add("Main.cirka".to_string(), text.clone());
    let opts = Options {
        tests: mode,
        ..Default::default()
    };
    resolve_program(&mut sm, id, &text, &mut MemLoader(map), &opts).map_err(|errs| {
        errs.iter()
            .map(|e| format!("{}: {}", e.code.clone().unwrap_or_default(), e.message))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[test]
fn collects_tests_of_all_modules_and_generates_main() {
    let files = [
        ("Prelude", TEST_PRELUDE),
        (
            "Lib",
            "module Lib\nprivate k : Int\nk = 1\ntest \"lib\" = pure ()\n",
        ),
        (
            "Main",
            "import Lib\nmain : IO ()\nmain = pure ()\ntest \"main\" = pure ()\n",
        ),
    ];
    let p = run_tests(&files, TestMode::Strip).unwrap();
    assert!(p.tests.is_empty());
    assert_eq!(p.info(p.main.unwrap()).name, "main");
    let p = run_tests(&files, TestMode::Check).unwrap();
    let names: Vec<(&str, &str)> = p
        .tests
        .iter()
        .map(|t| (t.module.as_str(), t.name.as_str()))
        .collect();
    assert_eq!(names, [("Lib", "lib"), ("Main", "main")]);
    assert_eq!(p.info(p.main.unwrap()).name, "main");
    let p = run_tests(&files, TestMode::Run).unwrap();
    let m = p.info(p.main.unwrap());
    assert_eq!((m.module.as_str(), m.name.as_str()), ("Main", "main$test"));
    // The test main refers to the private test definitions of `Lib`.
    let main_def = p
        .modules
        .iter()
        .flat_map(|m| &m.items)
        .find_map(|it| match it {
            Item::Def(d) if Some(d.name) == p.main => Some(d),
            _ => None,
        })
        .unwrap();
    let mut uses = Vec::new();
    global_uses(&main_def.clauses[0].rhs, &mut uses);
    for t in &p.tests {
        assert!(uses.iter().any(|(g, _)| *g == t.def), "{}", t.name);
    }
}

#[test]
fn test_only_names_are_rejected_outside_tests() {
    let files = [
        ("Prelude", TEST_PRELUDE),
        (
            "Main",
            "main : IO ()\nmain = pure ()\nf : Int\nf = h\ntests where\n  h : Int\n  h = 1\n",
        ),
    ];
    let e = run_tests(&files, TestMode::Check).unwrap_err();
    assert!(e.contains("E1113"), "{e}");
    // In normal builds the name does not exist at all.
    let e = run_tests(&files, TestMode::Strip).unwrap_err();
    assert!(e.contains("E1101"), "{e}");
}
