//! cirka-smt: the SMT solver (Z3) used by the verifiers
//! (13-verification.md §4).
//!
//! Formulas are built as [`Expr`] trees over integers and booleans and sent
//! to an external `z3` process as SMT-LIB 2 text (no linking to libz3).
//! The solver is found through `CIRKA_Z3` (a path or a command name; `z3`
//! when unset, disabled when empty), with the per-query timeout
//! `CIRKA_VERIFY_TIMEOUT` (milliseconds). A query asks whether hypotheses
//! imply a goal: it is valid when `hyps ∧ ¬goal` is unsatisfiable;
//! otherwise the model is a counterexample. Decided queries can be cached
//! in a directory (`_build/<name>/verify/`), keyed by their script.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The sort of a variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sort {
    Int,
    Bool,
}

/// A term of SMT-LIB's integer and boolean theories.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Expr {
    Var(String, Sort),
    Int(i128),
    Bool(bool),
    /// An operator of SMT-LIB (`+`, `-`, `*`, `div`, `mod`, `abs`, `=`,
    /// `<`, `<=`, `and`, `or`, `not`, `=>`, `ite`, ...).
    App(&'static str, Vec<Expr>),
    /// An uninterpreted function with its result sort (its argument sorts
    /// are those of the arguments).
    Fun(String, Vec<Expr>, Sort),
}

impl Expr {
    pub fn int_var(name: impl Into<String>) -> Expr {
        Expr::Var(name.into(), Sort::Int)
    }

    pub fn bool_var(name: impl Into<String>) -> Expr {
        Expr::Var(name.into(), Sort::Bool)
    }

    pub fn app(op: &'static str, args: Vec<Expr>) -> Expr {
        Expr::App(op, args)
    }

    pub fn negation(e: Expr) -> Expr {
        Expr::App("not", vec![e])
    }

    pub fn and(es: Vec<Expr>) -> Expr {
        match es.len() {
            0 => Expr::Bool(true),
            1 => es.into_iter().next().unwrap_or(Expr::Bool(true)),
            _ => Expr::App("and", es),
        }
    }

    pub fn eq(a: Expr, b: Expr) -> Expr {
        Expr::App("=", vec![a, b])
    }

    pub fn lt(a: Expr, b: Expr) -> Expr {
        Expr::App("<", vec![a, b])
    }

    pub fn le(a: Expr, b: Expr) -> Expr {
        Expr::App("<=", vec![a, b])
    }

    pub fn plus(a: Expr, b: Expr) -> Expr {
        Expr::App("+", vec![a, b])
    }

    pub fn minus(a: Expr, b: Expr) -> Expr {
        Expr::App("-", vec![a, b])
    }

    pub fn ite(c: Expr, t: Expr, e: Expr) -> Expr {
        Expr::App("ite", vec![c, t, e])
    }

    /// Truncating division (the semantics of `Int` `/`): SMT-LIB's `div`
    /// is Euclidean, so it is applied to the absolute values.
    pub fn tdiv(a: Expr, b: Expr) -> Expr {
        let q = Expr::app(
            "div",
            vec![
                Expr::app("abs", vec![a.clone()]),
                Expr::app("abs", vec![b.clone()]),
            ],
        );
        let same_sign = Expr::eq(Expr::le(Expr::Int(0), a), Expr::le(Expr::Int(0), b));
        Expr::ite(same_sign, q.clone(), Expr::app("-", vec![q]))
    }

    /// The remainder of truncating division (`Int` `%`): it has the sign of
    /// the dividend.
    pub fn trem(a: Expr, b: Expr) -> Expr {
        let r = Expr::app(
            "mod",
            vec![Expr::app("abs", vec![a.clone()]), Expr::app("abs", vec![b])],
        );
        Expr::ite(
            Expr::le(Expr::Int(0), a),
            r.clone(),
            Expr::app("-", vec![r]),
        )
    }

    /// The sort of the term.
    pub fn sort(&self) -> Sort {
        match self {
            Expr::Var(_, s) | Expr::Fun(_, _, s) => *s,
            Expr::Int(_) => Sort::Int,
            Expr::Bool(_) => Sort::Bool,
            Expr::App(op, args) => match *op {
                "=" | "<" | "<=" | ">" | ">=" | "and" | "or" | "not" | "=>" | "distinct" => {
                    Sort::Bool
                }
                "ite" => args.get(1).map(Expr::sort).unwrap_or(Sort::Int),
                _ => Sort::Int,
            },
        }
    }

    fn vars(
        &self,
        out: &mut BTreeSet<(String, Sort)>,
        funs: &mut BTreeSet<(String, Vec<Sort>, Sort)>,
    ) {
        match self {
            Expr::Var(x, s) => {
                out.insert((x.clone(), *s));
            }
            Expr::App(_, args) => {
                for a in args {
                    a.vars(out, funs);
                }
            }
            Expr::Fun(f, args, s) => {
                funs.insert((f.clone(), args.iter().map(Expr::sort).collect(), *s));
                for a in args {
                    a.vars(out, funs);
                }
            }
            Expr::Int(_) | Expr::Bool(_) => {}
        }
    }

    fn write(&self, out: &mut String) {
        match self {
            Expr::Var(x, _) => out.push_str(&quote_symbol(x)),
            Expr::Int(n) if *n < 0 => {
                let _ = write!(out, "(- {})", n.unsigned_abs());
            }
            Expr::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Expr::Bool(b) => {
                let _ = write!(out, "{b}");
            }
            Expr::App(op, args) => {
                out.push('(');
                out.push_str(op);
                for a in args {
                    out.push(' ');
                    a.write(out);
                }
                out.push(')');
            }
            Expr::Fun(f, args, _) if args.is_empty() => out.push_str(&quote_symbol(f)),
            Expr::Fun(f, args, _) => {
                out.push('(');
                out.push_str(&quote_symbol(f));
                for a in args {
                    out.push(' ');
                    a.write(out);
                }
                out.push(')');
            }
        }
    }
}

fn sort_name(s: Sort) -> &'static str {
    match s {
        Sort::Int => "Int",
        Sort::Bool => "Bool",
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        self.write(&mut s);
        f.write_str(&s)
    }
}

fn quote_symbol(x: &str) -> String {
    let simple = !x.is_empty()
        && !x.starts_with(|c: char| c.is_ascii_digit())
        && x.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.$!?-".contains(c));
    if simple {
        x.to_string()
    } else {
        format!("|{}|", x.replace(['|', '\\'], "_"))
    }
}

/// `hyps ⇒ goal`?
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub hyps: Vec<Expr>,
    pub goal: Option<Expr>,
}

impl Query {
    /// The SMT-LIB script: the declarations, the hypotheses and the negated
    /// goal, then `check-sat` and the values of the variables.
    pub fn script(&self) -> String {
        let mut vars = BTreeSet::new();
        let mut funs = BTreeSet::new();
        for h in &self.hyps {
            h.vars(&mut vars, &mut funs);
        }
        if let Some(g) = &self.goal {
            g.vars(&mut vars, &mut funs);
        }
        let mut s = String::new();
        for (f, args, res) in &funs {
            let args: Vec<&str> = args.iter().map(|a| sort_name(*a)).collect();
            let _ = writeln!(
                s,
                "(declare-fun {} ({}) {})",
                quote_symbol(f),
                args.join(" "),
                sort_name(*res)
            );
        }
        for (x, sort) in &vars {
            let _ = writeln!(
                s,
                "(declare-const {} {})",
                quote_symbol(x),
                sort_name(*sort)
            );
        }
        for h in &self.hyps {
            let _ = writeln!(s, "(assert {h})");
        }
        if let Some(g) = &self.goal {
            let _ = writeln!(s, "(assert (not {g}))");
        }
        s.push_str("(check-sat)\n");
        if !vars.is_empty() {
            s.push_str("(get-value (");
            for (i, (x, _)) in vars.iter().enumerate() {
                if i > 0 {
                    s.push(' ');
                }
                s.push_str(&quote_symbol(x));
            }
            s.push_str("))\n");
        }
        s
    }
}

/// The answer to a [`Query`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The goal follows from the hypotheses.
    Valid,
    /// A counterexample: values of the variables.
    Invalid(Vec<(String, String)>),
    /// The solver gave up (timeout, nonlinear arithmetic, ...).
    Unknown(String),
}

/// An external `z3` process.
#[derive(Clone, Debug)]
pub struct Solver {
    pub program: String,
    /// Per query, in milliseconds.
    pub timeout_ms: u64,
    /// Where decided queries are remembered.
    pub cache: Option<PathBuf>,
}

impl Solver {
    /// The solver named by `CIRKA_Z3` (`z3` when unset; none when empty),
    /// if it can be run.
    pub fn from_env() -> Option<Solver> {
        let program = match std::env::var("CIRKA_Z3") {
            Ok(s) if s.is_empty() => return None,
            Ok(s) => s,
            Err(_) => "z3".to_string(),
        };
        let timeout_ms = std::env::var("CIRKA_VERIFY_TIMEOUT")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(10_000);
        let ok = Command::new(&program)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        ok.then_some(Solver {
            program,
            timeout_ms,
            cache: None,
        })
    }

    /// Remembers decided queries in `dir`.
    pub fn with_cache(mut self, dir: PathBuf) -> Solver {
        self.cache = Some(dir);
        self
    }

    /// Decides a query (from the cache when it was decided before). `Err`
    /// when the solver cannot be run or answers something unexpected.
    pub fn prove(&self, q: &Query) -> Result<Outcome, String> {
        let script = q.script();
        let file = self
            .cache
            .as_ref()
            .map(|d| d.join(format!("{:016x}.smt2.out", fnv1a(&script))));
        if let Some(f) = &file
            && let Ok(text) = std::fs::read_to_string(f)
            && let Some((key, answer)) = text.split_once("\n;; answer\n")
            && key == script
            && let Some(o) = Outcome::decode(answer)
        {
            return Ok(o);
        }
        let o = self.run(&script)?;
        if let Some(f) = &file
            && !matches!(o, Outcome::Unknown(_))
        {
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(f, format!("{script}\n;; answer\n{}", o.encode()));
        }
        Ok(o)
    }

    fn run(&self, script: &str) -> Result<Outcome, String> {
        let mut child = Command::new(&self.program)
            .arg("-in")
            .arg("-smt2")
            .arg(format!("-t:{}", self.timeout_ms))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run `{}`: {e}", self.program))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(script.as_bytes())
                .map_err(|e| format!("cannot write to `{}`: {e}", self.program))?;
        }
        let out = child
            .wait_with_output()
            .map_err(|e| format!("`{}` failed: {e}", self.program))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        match lines.next().map(str::trim) {
            Some("unsat") => Ok(Outcome::Valid),
            Some("sat") => Ok(Outcome::Invalid(parse_values(
                &lines.collect::<Vec<_>>().join(" "),
            ))),
            Some("unknown") => Ok(Outcome::Unknown("the solver returned `unknown`".into())),
            Some("timeout") => Ok(Outcome::Unknown("timeout".into())),
            other => Err(format!(
                "unexpected answer from `{}`: {}{}",
                self.program,
                other.unwrap_or(""),
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        }
    }
}

impl Outcome {
    fn encode(&self) -> String {
        match self {
            Outcome::Valid => "valid\n".into(),
            Outcome::Invalid(m) => {
                let mut s = String::from("invalid\n");
                for (k, v) in m {
                    let _ = writeln!(s, "{k}\t{v}");
                }
                s
            }
            Outcome::Unknown(why) => format!("unknown\t{why}\n"),
        }
    }

    fn decode(s: &str) -> Option<Outcome> {
        let mut lines = s.lines();
        match lines.next()? {
            "valid" => Some(Outcome::Valid),
            "invalid" => Some(Outcome::Invalid(
                lines
                    .filter_map(|l| l.split_once('\t'))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            )),
            _ => None,
        }
    }
}

/// FNV-1a (stable across builds, unlike `DefaultHasher`).
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ---- s-expressions (for `get-value`) ----

#[derive(Debug)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

fn parse_sexp(toks: &[String], i: &mut usize) -> Option<Sexp> {
    let t = toks.get(*i)?;
    *i += 1;
    if t == "(" {
        let mut items = Vec::new();
        while toks.get(*i).is_some_and(|t| t != ")") {
            items.push(parse_sexp(toks, i)?);
        }
        *i += 1;
        Some(Sexp::List(items))
    } else {
        Some(Sexp::Atom(t.clone()))
    }
}

fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_bar = false;
    for c in s.chars() {
        if in_bar {
            cur.push(c);
            if c == '|' {
                in_bar = false;
            }
            continue;
        }
        match c {
            '(' | ')' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            '|' => {
                in_bar = true;
                cur.push(c);
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn show_value(s: &Sexp) -> String {
    match s {
        Sexp::Atom(a) => a.clone(),
        Sexp::List(items) => match items.as_slice() {
            [Sexp::Atom(m), x] if m == "-" => format!("-{}", show_value(x)),
            _ => {
                let parts: Vec<String> = items.iter().map(show_value).collect();
                format!("({})", parts.join(" "))
            }
        },
    }
}

/// `((x 1) (y (- 2)))` → `[(x, 1), (y, -2)]`.
fn parse_values(s: &str) -> Vec<(String, String)> {
    let toks = tokens(s);
    let mut i = 0;
    let Some(Sexp::List(pairs)) = parse_sexp(&toks, &mut i) else {
        return Vec::new();
    };
    pairs
        .iter()
        .filter_map(|p| match p {
            Sexp::List(kv) if kv.len() == 2 => {
                let k = match &kv[0] {
                    Sexp::Atom(a) => a.trim_matches('|').to_string(),
                    _ => return None,
                };
                Some((k, show_value(&kv[1])))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_declares_asserts_and_negates_the_goal() {
        let x = Expr::int_var("x");
        let q = Query {
            hyps: vec![Expr::lt(Expr::Int(0), x.clone())],
            goal: Some(Expr::le(Expr::Int(-1), x)),
        };
        assert_eq!(
            q.script(),
            "(declare-const x Int)\n(assert (< 0 x))\n(assert (not (<= (- 1) x)))\n\
             (check-sat)\n(get-value (x))\n"
        );
    }

    #[test]
    fn symbols_are_quoted_when_needed() {
        assert_eq!(quote_symbol("a.b$1"), "a.b$1");
        assert_eq!(quote_symbol("x 1"), "|x 1|");
        assert_eq!(quote_symbol("1x"), "|1x|");
    }

    #[test]
    fn values_are_parsed() {
        assert_eq!(
            parse_values("((x 1)\n (|y z| (- 2)))"),
            vec![("x".into(), "1".into()), ("y z".into(), "-2".into())]
        );
    }

    /// Runs only where Z3 is installed (the Docker image).
    #[test]
    fn z3_decides_simple_queries() {
        let Some(s) = Solver::from_env() else {
            eprintln!("z3 not found: skipped");
            return;
        };
        let a = Expr::int_var("a");
        let b = Expr::int_var("b");
        // b ≠ 0 ⇒ |a % b| < |b|
        let q = Query {
            hyps: vec![Expr::negation(Expr::eq(b.clone(), Expr::Int(0)))],
            goal: Some(Expr::lt(
                Expr::app("abs", vec![Expr::trem(a.clone(), b.clone())]),
                Expr::app("abs", vec![b.clone()]),
            )),
        };
        assert_eq!(s.prove(&q), Ok(Outcome::Valid));
        // a - 1 < a, but not a < a - 1.
        let q = Query {
            hyps: vec![],
            goal: Some(Expr::lt(a.clone(), Expr::minus(a.clone(), Expr::Int(1)))),
        };
        assert!(matches!(s.prove(&q), Ok(Outcome::Invalid(m)) if m.iter().any(|(k, _)| k == "a")));
        // Uninterpreted functions: f a = f a, but not f a = f b.
        let fa = Expr::Fun("f".into(), vec![a.clone()], Sort::Int);
        let fb = Expr::Fun("f".into(), vec![b.clone()], Sort::Int);
        let q = Query {
            hyps: vec![Expr::eq(a.clone(), b.clone())],
            goal: Some(Expr::eq(fa.clone(), fb.clone())),
        };
        assert_eq!(s.prove(&q), Ok(Outcome::Valid));
        // The cache answers the second time without the solver.
        let dir = std::env::temp_dir().join(format!("cirka-smt-cache-{}", std::process::id()));
        let cached = s.clone().with_cache(dir.clone());
        let q = Query {
            hyps: vec![],
            goal: Some(Expr::eq(fa, fb)),
        };
        let first = cached.prove(&q);
        assert!(matches!(first, Ok(Outcome::Invalid(_))));
        let broken = Solver {
            program: "/nonexistent/z3".into(),
            ..cached
        };
        assert_eq!(broken.prove(&q), first);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn outcomes_round_trip_through_the_cache_format() {
        let o = Outcome::Invalid(vec![("x".into(), "-1".into())]);
        assert_eq!(Outcome::decode(&o.encode()), Some(o));
        assert_eq!(
            Outcome::decode(&Outcome::Valid.encode()),
            Some(Outcome::Valid)
        );
    }

    #[test]
    fn uninterpreted_functions_are_declared() {
        let q = Query {
            hyps: vec![],
            goal: Some(Expr::lt(
                Expr::Fun("len".into(), vec![Expr::int_var("xs")], Sort::Int),
                Expr::Int(3),
            )),
        };
        assert!(
            q.script()
                .starts_with("(declare-fun len (Int) Int)\n(declare-const xs Int)\n")
        );
    }
}
