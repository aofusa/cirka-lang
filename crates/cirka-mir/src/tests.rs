use crate::*;
use cirka_diagnostics::{FileId, Span};

pub(crate) const MAP_EXAMPLE: &str = r#"
type List { Nil = 0 {}, Cons = 1 { head: ptr, tail: ptr } }

fn map(f: ptr @owned, xs: ptr @owned) -> ptr {
entry:
  switch xs {
    List.Nil  => nil_case,
    List.Cons => cons_case,
  }
nil_case:
  drop f
  ret xs                              // Nil is a static object
cons_case:
  let h  = proj xs List.Cons.head
  let t  = proj xs List.Cons.tail
  dup h
  dup t
  let tok = drop_reuse xs             // reuse xs when RC==1
  dup f
  let y  = apply f(h)
  let ys = call map(f, t)
  let r  = alloc List.Cons(y, ys) reuse tok
  ret r
}
"#;

fn strip_spans(mut m: Module) -> Module {
    for f in &mut m.funcs {
        f.span = Span::DUMMY;
    }
    m
}

#[test]
fn parses_design_example() {
    let m = parse(FileId(0), MAP_EXAMPLE).unwrap();
    assert_eq!(m.types.len(), 1);
    let f = m.func("map").unwrap();
    assert_eq!(f.blocks.len(), 3);
    assert_eq!(f.params[1].own, Own::Owned);
    let printed = print_module(&m);
    let again = parse(FileId(0), &printed).unwrap();
    assert_eq!(strip_spans(m), strip_spans(again));
}

#[test]
fn parse_errors_have_spans() {
    let e = parse(FileId(0), "fn f() -> i64 {\nentry:\n  ret y\n}").unwrap_err();
    assert!(
        e.message.contains("undefined variable `y`"),
        "{}",
        e.message
    );
    assert!(e.primary_span().is_some());
    let e = parse(
        FileId(0),
        "fn f() -> i64 { entry: let x = prim nope(1) ret x }",
    )
    .unwrap_err();
    assert!(e.message.contains("unknown primitive"));
    let e = parse(
        FileId(0),
        "fn f() -> i64 { entry: let x = lit 1 let x = lit 2 ret x }",
    )
    .unwrap_err();
    assert!(e.message.contains("more than once"));
    assert!(parse(FileId(0), "fn f(").is_err());
    assert!(parse(FileId(0), "\"abc").is_err());
}

// ---- property test: print → parse round trip on random modules ----

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn random_module(rng: &mut Rng) -> Module {
    let mut m = Module::default();
    let nt = 1 + rng.below(3);
    for t in 0..nt {
        let nc = 1 + rng.below(3);
        let ctors = (0..nc)
            .map(|c| CtorDef {
                name: format!("C{c}"),
                tag: c as u32,
                fields: (0..rng.below(4))
                    .map(|i| Field {
                        name: format!("f{i}"),
                        ty: [Ty::Ptr, Ty::I64, Ty::F64, Ty::I32][rng.below(4)],
                    })
                    .collect(),
            })
            .collect();
        m.types.push(TypeDef {
            name: format!("T{t}"),
            ctors,
        });
    }
    m.externs.push(ExternDecl {
        name: "ext".into(),
        params: vec![(Ty::I64, Own::Borrowed), (Ty::Ptr, Own::Owned)],
        ret: if rng.below(2) == 0 {
            None
        } else {
            Some(Ty::I64)
        },
        c_sig: None,
    });
    let nf = 1 + rng.below(3);
    for fi in 0..nf {
        let mut f = Function::new(format!("fn{fi}"), Ty::I64);
        let mut ints = Vec::new();
        let mut ptrs = Vec::new();
        for i in 0..rng.below(3) {
            let ty = if i % 2 == 0 { Ty::I64 } else { Ty::Ptr };
            let v = f.new_var(format!("p{i}"), ty);
            f.params.push(Param {
                var: v,
                // Scalars are always passed by value (printed without @).
                own: if rng.below(2) == 0 || !ty.is_ptr() {
                    Own::Owned
                } else {
                    Own::Borrowed
                },
            });
            if ty == Ty::Ptr {
                ptrs.push(v)
            } else {
                ints.push(v)
            }
        }
        let nb = 1 + rng.below(3);
        let mut blocks = Vec::new();
        for bi in 0..nb {
            let mut instrs = Vec::new();
            for k in 0..rng.below(6) {
                let pick = rng.below(9);
                let iop = |rng: &mut Rng, ints: &Vec<Var>| {
                    if !ints.is_empty() && rng.below(2) == 0 {
                        Operand::Var(ints[rng.below(ints.len())])
                    } else {
                        Operand::Const(Const::Int(rng.next() as i32 as i64, Ty::I64))
                    }
                };
                match pick {
                    0 => {
                        let v = f.new_var(format!("x{bi}_{k}"), Ty::I64);
                        let a = iop(rng, &ints);
                        let b = iop(rng, &ints);
                        instrs.push(Instr::Let {
                            var: Some(v),
                            rhs: Rhs::Prim(Prim::ALL[rng.below(18)], vec![a, b]).fix_arity(),
                        });
                        ints.push(v);
                    }
                    1 => {
                        let v = f.new_var(format!("s{bi}_{k}"), Ty::Ptr);
                        let bytes: Vec<u8> =
                            (0..rng.below(5)).map(|_| rng.below(256) as u8).collect();
                        instrs.push(Instr::Let {
                            var: Some(v),
                            rhs: Rhs::Lit(Const::Str(bytes)),
                        });
                        ptrs.push(v);
                    }
                    2 if !ptrs.is_empty() => instrs.push(Instr::Dup(ptrs[rng.below(ptrs.len())])),
                    3 if !ptrs.is_empty() => instrs.push(Instr::Drop(ptrs[rng.below(ptrs.len())])),
                    4 => {
                        let v = f.new_var(format!("d{bi}_{k}"), Ty::F64);
                        let x = (rng.next() % 10000) as f64 / 8.0;
                        instrs.push(Instr::Let {
                            var: Some(v),
                            rhs: Rhs::Lit(Const::Float(x)),
                        });
                    }
                    5 => {
                        let v = f.new_var(format!("w{bi}_{k}"), Ty::I32);
                        instrs.push(Instr::Let {
                            var: Some(v),
                            rhs: Rhs::Lit(Const::Int(-(rng.below(100) as i64), Ty::I32)),
                        });
                    }
                    6 if !ptrs.is_empty() => {
                        let v = f.new_var(format!("t{bi}_{k}"), Ty::Ptr);
                        instrs.push(Instr::Let {
                            var: Some(v),
                            rhs: Rhs::Force(ptrs[rng.below(ptrs.len())]),
                        });
                        ptrs.push(v);
                    }
                    7 => {
                        let t = rng.below(m.types.len());
                        let c = rng.below(m.types[t].ctors.len());
                        let cr = CtorRef {
                            ty: t as u32,
                            ctor: c as u32,
                        };
                        let args = m
                            .ctor(cr)
                            .fields
                            .iter()
                            .map(|fd| match fd.ty {
                                Ty::Ptr if !ptrs.is_empty() => Operand::Var(ptrs[0]),
                                Ty::Ptr => Operand::Const(Const::Str(b"z".to_vec())),
                                Ty::F64 => Operand::Const(Const::Float(2.5)),
                                t => Operand::Const(Const::Int(7, t)),
                            })
                            .collect();
                        let v = f.new_var(format!("a{bi}_{k}"), Ty::Ptr);
                        instrs.push(Instr::Let {
                            var: Some(v),
                            rhs: Rhs::Alloc {
                                ctor: cr,
                                args,
                                reuse: None,
                            },
                        });
                        ptrs.push(v);
                    }
                    8 if !ptrs.is_empty() => {
                        let a = Operand::Var(ptrs[0]);
                        let var = if m.externs[0].ret.is_some() {
                            Some(f.new_var(format!("e{bi}_{k}"), Ty::I64))
                        } else {
                            None
                        };
                        instrs.push(Instr::Let {
                            var,
                            rhs: Rhs::Extern {
                                func: "ext".into(),
                                args: vec![Operand::Const(Const::Int(1, Ty::I64)), a],
                            },
                        });
                    }
                    _ => {}
                }
            }
            let term = if bi + 1 < nb {
                match rng.below(2) {
                    0 => Terminator::Jump(BlockId(bi as u32 + 1), vec![]),
                    _ if !ints.is_empty() => Terminator::Switch {
                        scrut: ints[0],
                        arms: vec![(Case::Int(rng.below(5) as i64 - 2), BlockId(bi as u32 + 1))],
                        default: Some(BlockId(nb as u32 - 1)),
                    },
                    _ => Terminator::Panic("bad\n\"x\"".into()),
                }
            } else if !ints.is_empty() {
                Terminator::Ret(Operand::Var(ints[rng.below(ints.len())]))
            } else {
                Terminator::Ret(Operand::Const(Const::Int(-3, Ty::I64)))
            };
            blocks.push(Block {
                label: format!("b{bi}"),
                params: vec![],
                instrs,
                term,
            });
        }
        f.blocks = blocks;
        m.funcs.push(f);
    }
    m
}

impl Rhs {
    fn fix_arity(self) -> Rhs {
        match self {
            Rhs::Prim(p, mut a) => {
                a.truncate(p.signature().0.len());
                Rhs::Prim(p, a)
            }
            r => r,
        }
    }
}

#[test]
fn roundtrip_property() {
    let mut rng = Rng(0x2545F4914F6CDD1D);
    for _ in 0..500 {
        let m = random_module(&mut rng);
        let text = print_module(&m);
        let parsed = parse(FileId(0), &text).unwrap_or_else(|e| panic!("{}\n{text}", e.message));
        assert_eq!(strip_spans(parsed.clone()), strip_spans(m), "\n{text}");
        assert_eq!(print_module(&parsed), text);
    }
}

#[test]
fn verifier_accepts_design_example() {
    let m = parse(FileId(0), MAP_EXAMPLE).unwrap();
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
}

fn interp(src: &str) -> Result<crate::interp::RunResult, String> {
    let m = parse(FileId(0), src).unwrap();
    crate::interp::run(&m, b"", 1_000_000)
}

#[test]
fn interp_detects_double_free() {
    let e = interp("type B { B = 0 { v: i64 } }\nfn main() -> i64 { entry: let b = alloc B.B(1) drop b drop b ret 0 }").unwrap_err();
    assert!(e.contains("double free"), "{e}");
}

#[test]
fn interp_detects_use_after_free() {
    let e = interp("type B { B = 0 { v: i64 } }\nfn main() -> i64 { entry: let b = alloc B.B(1) drop b let v = proj b B.B.v ret v }").unwrap_err();
    assert!(e.contains("use after free"), "{e}");
}

#[test]
fn interp_reports_leaks_and_stats() {
    let r = interp("type B { B = 0 { v: i64 } }\nfn main() -> i64 { entry: let b = alloc B.B(1) let c = alloc B.B(2) drop c ret 0 }").unwrap();
    assert_eq!(r.leaked, 1);
    assert_eq!(r.stats.allocs, 2);
    assert_eq!(r.stats.frees, 1);
}

#[test]
fn interp_reuse_map_has_no_new_allocations() {
    let src = format!(
        "{MAP_EXAMPLE}\nextern fn cirka_write_i64(i64) -> void
fn id(x: ptr @owned) -> ptr {{ entry: ret x }}
fn main() -> i64 {{
entry:
  let n = alloc List.Nil()
  let a = alloc List.Cons(n, n)
  let b = alloc List.Cons(n, a)
  let f = mkclosure id()
  let r = call map(f, b)
  drop r
  ret 0
}}"
    );
    let r = interp(&src).unwrap();
    // 2 cells + 1 closure; map reuses both cells in place.
    assert_eq!(r.stats.allocs, 3, "{:?}", r.stats);
    assert_eq!(r.stats.reuses, 2);
    assert_eq!(r.leaked, 0);
}

#[test]
fn c_signatures_round_trip_and_run() {
    let src = "extern fn abs(i64) -> i64 as c(i32) -> i32\n\
               extern fn ldexpf(f64, i64) -> f64 as c(f32, i32) -> f32\n\
               extern fn cirka_write_i64(i64) -> void\n\
               fn main() -> i64 {\n\
               entry:\n\
               let a = extern abs(-5)\n\
               let _ = extern cirka_write_i64(a)\n\
               ret 0\n\
               }\n";
    let m = parse(FileId(0), src).unwrap();
    let d = m.extern_decl("ldexpf").unwrap();
    assert_eq!(
        d.c_sig.as_ref().unwrap().params,
        vec![crate::CTy::F32, crate::CTy::I32]
    );
    let printed = print_module(&m);
    assert!(printed.contains("as c(f32, i32) -> f32"), "{printed}");
    let again = parse(FileId(0), &printed).unwrap();
    assert_eq!(strip_spans(m.clone()), strip_spans(again));
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stdout, "5");
}

// ---- multiple results (FEAT-0065) ----

const MULTI: &str = "type Box { B = 0 { v: i64 } }

fn pair(x: ptr @borrowed, n: i64) -> (ptr, i64) {
entry:
  let v = proj x Box.B.v
  let b = alloc Box.B(n)
  ret (b, v)
}

fn again(x: ptr @borrowed, n: i64) -> (ptr, i64) {
entry:
  let (b, v) = call pair(x, n)
  ret (b, v)
}

fn main() -> i64 {
entry:
  let a = alloc Box.B(3)
  let (b, v) = call again(a, 4)
  drop a
  let w = proj b Box.B.v
  drop b
  let r = prim add(v, w)
  ret r
}
";

fn verify_errors(src: &str) -> String {
    let m = parse(FileId(0), src).unwrap_or_else(|e| panic!("{}", e.message));
    crate::verify::render_errors(&crate::verify::verify_module(&m))
}

#[test]
fn multiple_results_round_trip_and_run() {
    let m = parse(FileId(0), MULTI).unwrap();
    let f = m.func("pair").unwrap();
    assert_eq!(f.rets, vec![Ty::Ptr, Ty::I64]);
    assert_eq!(f.results(), &[Ty::Ptr, Ty::I64]);
    assert!(f.is_multi());
    assert_eq!(m.func("main").unwrap().results(), &[Ty::I64]);
    let printed = print_module(&m);
    assert!(printed.contains("fn pair(x: ptr @borrowed, n: i64) -> (ptr, i64) {"));
    assert!(
        printed.contains("let (b, v) = call pair(x, n)"),
        "{printed}"
    );
    assert!(printed.contains("ret (b, v)"), "{printed}");
    let again = parse(FileId(0), &printed).unwrap();
    assert_eq!(strip_spans(m.clone()), strip_spans(again));
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.exit_code, 7);
    assert_eq!(r.leaked, 0);
    // Perceus from scratch gives the same result.
    let mut m2 = m.clone();
    crate::perceus::strip_rc(&mut m2);
    crate::perceus::run(&mut m2);
    let errs = crate::verify::verify_module(&m2);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let r = crate::interp::run(&m2, b"", 1_000_000).unwrap();
    assert_eq!((r.exit_code, r.leaked), (7, 0));
}

#[test]
fn multiple_results_parse_errors() {
    let e = parse(FileId(0), "fn f() -> (i64) { entry: ret 1 }").unwrap_err();
    assert!(e.message.contains("at least two"), "{}", e.message);
    let e = parse(
        FileId(0),
        "fn f() -> i64 { entry: let (a, b) = call g() ret a }",
    )
    .unwrap_err();
    assert!(e.message.contains("unknown function `g`"), "{}", e.message);
}

#[test]
fn verifier_rejects_wrong_multiple_results() {
    let pair = "fn pair(n: i64) -> (ptr, i64) {
entry:
  let s = lit \"s\"
  ret (s, n)
}
";
    let main0 = "fn main() -> i64 { entry: ret 0 }";
    // Wrong arity of the binding.
    let e = verify_errors(&format!(
        "{pair}fn main() -> i64 {{ entry: let (a, b, c) = call pair(1) drop a ret b }}"
    ));
    assert!(e.contains("returns 2 values, but 3 variables"), "{e}");
    // A single binding of a multi-result call and vice versa.
    let e = verify_errors(&format!(
        "{pair}fn main() -> i64 {{ entry: let a = call pair(1) drop a ret 0 }}"
    ));
    assert!(
        e.contains("returns 2 values; bind them with `let (..)"),
        "{e}"
    );
    let e = verify_errors(
        "fn one() -> i64 { entry: ret 1 }
fn main() -> i64 { entry: let (a, b) = call one() ret a }",
    );
    assert!(e.contains("returns a single value"), "{e}");
    // `ret` arity and types.
    let e = verify_errors(&format!(
        "fn f(n: i64) -> (ptr, i64) {{ entry: ret (n, n) }}\n{main0}"
    ));
    assert!(e.contains("`n` has type i64, expected ptr"), "{e}");
    let e = verify_errors(&format!(
        "fn f(n: i64) -> (i64, i64) {{ entry: ret (n, n, n) }}\n{main0}"
    ));
    assert!(
        e.contains("`ret` of 3 values in a function returning 2"),
        "{e}"
    );
    let e = verify_errors(&format!(
        "fn f(n: i64) -> (i64, i64) {{ entry: ret n }}\n{main0}"
    ));
    assert!(
        e.contains("`ret` of one value in a function returning 2 values"),
        "{e}"
    );
    let e = verify_errors(&format!(
        "fn f(n: i64) -> i64 {{ entry: ret (n, n) }}\n{main0}"
    ));
    assert!(
        e.contains("`ret (..)` in a function returning a single value"),
        "{e}"
    );
    // Tail calls need the same result types.
    let e = verify_errors(&format!(
        "{pair}fn g(n: i64) -> (i64, ptr) {{ entry: tailcall pair(n) }}\n{main0}"
    ));
    assert!(
        e.contains("tail call to `pair` returning (ptr, i64) from a function returning (i64, ptr)"),
        "{e}"
    );
    // Results are owned: they must be consumed exactly once.
    let e = verify_errors(&format!(
        "{pair}fn main() -> i64 {{ entry: let (a, b) = call pair(1) ret b }}"
    ));
    assert!(e.contains("owned `a` is not consumed"), "{e}");
    let e = verify_errors(&format!(
        "{pair}fn main() -> i64 {{ entry: let (a, b) = call pair(1) drop a drop a ret b }}"
    ));
    assert!(e.contains("`a` is consumed more than once"), "{e}");
    let e = verify_errors(&format!(
        "{pair}fn h(n: i64) -> (ptr, i64) {{ entry: let (a, b) = call pair(n) drop a ret (a, b) }}\n{main0}"
    ));
    assert!(e.contains("`a` is consumed more than once"), "{e}");
    // Closures and thunks return one value.
    let e = verify_errors(&format!(
        "{pair}fn main() -> i64 {{ entry: let t = mkthunk pair(1) drop t ret 0 }}"
    ));
    assert!(e.contains("thunk function `pair` must return ptr"), "{e}");
}

const UNIQUE_LOOP: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn fill(a: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let x = prim bytes_get_i64(a, i)
  let c = prim bytes_unique(a)
  let y = prim add(x, i)
  let s = prim bytes_set_i64(c, i, y)
  let j = prim add(i, 1)
  tailcall fill(c, j, n)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  dup a
  let b = call fill(a, 0, 4)
  let s1 = prim bytes_get_i64(a, 3)
  let s2 = prim bytes_get_i64(b, 3)
  drop a
  drop b
  let _ = extern cirka_write_i64(s1)
  let _ = extern cirka_write_i64(s2)
  ret 0
}
"#;

#[test]
fn unique_loops_are_specialized() {
    let before = parse(FileId(0), UNIQUE_LOOP).unwrap();
    let mut m = before.clone();
    assert_eq!(crate::uniq::specialize_loops(&mut m).len(), 1);
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let printed = print_module(&m);
    let f = m.func("fill").unwrap();
    // The original checks once and continues in the specialized function.
    assert!(printed.contains("tailcall fill$u(c, j, n)"), "{printed}");
    let u = m.func("fill$u").unwrap();
    assert_eq!(u.params, f.params);
    let text = print_module(&Module {
        funcs: vec![u.clone()],
        ..before.clone()
    });
    assert!(!text.contains("bytes_unique"), "{text}");
    assert!(text.contains("tailcall fill$u(a, j, n)"), "{text}");
    // Same output and allocations (one copy of the shared array), and the
    // interpreter's check that stores go to unique arrays passes.
    let r0 = crate::interp::run(&before, b"", 1_000_000).unwrap();
    let r1 = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r0.stdout, r1.stdout);
    assert_eq!(r0.stats.allocs, 2);
    assert_eq!(r1.stats.allocs, 2);
    assert_eq!(r1.leaked, 0);
}

#[test]
fn unique_loops_keep_checks_of_shared_values() {
    // The checked array is shared before the next iteration: not specialized.
    let src = UNIQUE_LOOP.replace(
        "  tailcall fill(c, j, n)",
        "  dup c\n  drop c\n  tailcall fill(c, j, n)",
    );
    let mut m = parse(FileId(0), &src).unwrap();
    assert_eq!(crate::uniq::specialize_loops(&mut m).len(), 0);
    // Paths that meet in a block: one of them shares the array, so nothing
    // is known afterwards, in whichever order the analysis visits them.
    for (zero, other) in [("clean", "dirty"), ("dirty", "clean")] {
        let src = UNIQUE_LOOP.replace(
            "  tailcall fill(c, j, n)",
            &format!(
                "  switch i {{ 0 => {zero}, default => {other} }}
clean:
  jump next()
dirty:
  dup c
  drop c
  jump next()
next:
  tailcall fill(c, j, n)"
            ),
        );
        let mut m = parse(FileId(0), &src).unwrap();
        assert_eq!(
            crate::uniq::specialize_loops(&mut m).len(),
            0,
            "{zero} first"
        );
    }
    // The checked array moves to another parameter: the next iteration
    // checks the other array, which nothing is known about.
    let src = "fn rot(a: ptr @owned, b: ptr @owned, i: i64) -> i64 {
entry:
  let d = prim ge(i, 4)
  switch d { 0 => body, default => done }
done:
  drop a
  drop b
  ret 0
body:
  let c = prim bytes_unique(a)
  let s = prim bytes_set_i64(c, 0, i)
  let j = prim add(i, 1)
  tailcall rot(b, c, j)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(8)
  dup a
  let r = call rot(a, a, 0)
  ret r
}";
    let mut m = parse(FileId(0), src).unwrap();
    assert_eq!(crate::uniq::specialize_loops(&mut m).len(), 0);
    assert!(crate::interp::run(&m, b"", 1_000_000).is_ok());
    // In-place reads keep it unique.
    let src = UNIQUE_LOOP.replace(
        "  let c = prim bytes_unique(a)\n",
        "  let c = prim bytes_unique(a)\n  let k = prim bytes_len(c)\n",
    );
    let mut m = parse(FileId(0), &src).unwrap();
    assert_eq!(crate::uniq::specialize_loops(&mut m).len(), 1);
    // Only unchecked parameters reach the tail call: nothing is known.
    let src = UNIQUE_LOOP
        .replace("  let c = prim bytes_unique(a)\n", "")
        .replace("bytes_set_i64(c, i, y)", "bytes_get_i64(a, i)")
        .replace("tailcall fill(c, j, n)", "tailcall fill(a, j, n)");
    let mut m = parse(FileId(0), &src).unwrap();
    assert_eq!(crate::uniq::specialize_loops(&mut m).len(), 0);
}

/// A loop with all three routes: `skip` passes the array unchecked, `plain`
/// passes it checked, and `keep` shares it. `a` starts shared with `saved`.
const UNIQUE_ROUTES: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn run(a: ptr @owned, saved: ptr @owned, i: i64, n: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  let x = prim bytes_get_i64(a, 0)
  let y = prim bytes_get_i64(saved, 0)
  drop a
  drop saved
  let h = prim mul(x, 100)
  let r = prim add(h, y)
  ret r
body:
  let j = prim add(i, 1)
  let w = prim lt(i, 1)
  switch w { 0 => write, default => skip }
skip:
  tailcall run(a, saved, j, n)
write:
  let c = prim bytes_unique(a)
  let s = prim bytes_set_i64(c, 0, i)
  let e = prim eq(i, 2)
  switch e { 0 => plain, default => keep }
plain:
  tailcall run(c, saved, j, n)
keep:
  drop saved
  dup c
  tailcall run(c, c, j, n)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(8)
  dup a
  let r = call run(a, a, 0, 5)
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;

#[test]
fn unique_loops_enter_and_leave_the_specialized_function() {
    let before = parse(FileId(0), UNIQUE_ROUTES).unwrap();
    let mut m = before.clone();
    assert_eq!(crate::uniq::specialize_loops(&mut m).len(), 1);
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let text = |name: &str| {
        print_module(&Module {
            funcs: vec![m.func(name).unwrap().clone()],
            ..before.clone()
        })
    };
    // The original only enters `run$u` with a checked array.
    let f = text("run");
    assert!(f.contains("tailcall run(a, saved, j, n)"), "{f}");
    assert!(f.contains("tailcall run$u(c, saved, j, n)"), "{f}");
    assert!(f.contains("tailcall run(c, c, j, n)"), "{f}");
    // `run$u` loops while the array is unique and leaves once it is shared.
    let u = text("run$u");
    assert!(!u.contains("bytes_unique"), "{u}");
    assert_eq!(
        u.matches("tailcall run$u(a, saved, j, n)").count(),
        2,
        "{u}"
    );
    assert!(u.contains("tailcall run(a, a, j, n)"), "{u}");
    // i = 0 skips, 1 copies the shared array, 2 is saved and shared, 3 copies
    // again, 4 writes in place: a[0] = 4 and saved[0] = 2.
    let r0 = crate::interp::run(&before, b"", 1_000_000).unwrap();
    let r1 = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r1.stdout, "402");
    assert_eq!(r0.stdout, r1.stdout);
    assert_eq!(r0.stats.allocs, 3);
    assert_eq!(r1.stats.allocs, 3);
    assert_eq!(r1.leaked, 0);
}

// ---- FEAT-0069: bounds checks hoisted to the loop entry ----

/// Runs `hoist_checks` on `src` and checks that the result passes the
/// verifier and runs like the original in the interpreter, which faults
/// when an unchecked access is out of bounds (a wrong proof). Returns the
/// versioned module.
fn hoisted(src: &str, versioned: usize) -> Module {
    let before = parse(FileId(0), src).unwrap();
    let mut m = before.clone();
    let n = crate::bounds::hoist_checks(&mut m);
    let text = print_module(&m);
    assert_eq!(n, versioned, "{text}");
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let r0 = crate::interp::run(&before, b"", 1_000_000).unwrap();
    let r1 = crate::interp::run(&m, b"", 1_000_000).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(r0.stdout, r1.stdout, "{text}");
    assert_eq!(r0.stderr, r1.stderr, "{text}");
    assert_eq!(r0.exit_code, r1.exit_code);
    assert_eq!(r0.stats.allocs, r1.stats.allocs);
    assert_eq!(r0.leaked, r1.leaked);
    m
}

/// The precondition of `name` in `src`, sorted.
fn precondition(src: &str, name: &str) -> Vec<String> {
    let m = parse(FileId(0), src).unwrap();
    let mut pre = crate::bounds::precondition_of(m.func(name).unwrap()).unwrap_or_default();
    pre.sort();
    pre
}

fn func_text(m: &Module, name: &str) -> String {
    print_module(&Module {
        funcs: vec![m.func(name).unwrap().clone()],
        ..m.clone()
    })
}

/// The accesses in `text`: (unchecked, checked).
fn accesses(text: &str) -> (usize, usize) {
    let mut n = (0, 0);
    for p in Prim::ALL {
        if crate::bounds::is_access(*p) {
            let k = text.matches(&format!("prim {}(", p.name())).count();
            if p.is_unchecked() {
                n.0 += k;
            } else {
                n.1 += k;
            }
        }
    }
    n
}

const BOUNDS_FILL: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn fin(a: ptr @owned) -> ptr {
entry:
  ret a
}

fn fill(n: i64, a: ptr @owned, i: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  tailcall fin(a)
body:
  let k = prim mul(i, 3)
  let s = prim bytes_set_i64(a, i, k)
  let j = prim add(i, 1)
  tailcall fill(n, a, j)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let b = call fill(N, a, I)
  let x = prim bytes_get_i64(b, 3)
  drop b
  let _ = extern cirka_write_i64(x)
  ret 0
}
"#;

#[test]
fn bounds_checks_move_to_the_loop_entry() {
    let src = BOUNDS_FILL.replace("fill(N, a, I)", "fill(4, a, 0)");
    let m = parse(FileId(0), &src).unwrap();
    assert_eq!(
        crate::bounds::precondition_of(m.func("fill").unwrap()),
        Some(vec!["i >= 0".to_string(), "n <= words(a)".to_string()])
    );
    let m = hoisted(&src, 1);
    // `fill` tests the precondition once and continues in `fill$b`, which
    // stays there without checks.
    let f = func_text(&m, "fill");
    assert!(f.contains("tailcall fill$b(n, a, i)"), "{f}");
    assert!(f.contains("tailcall fill(n, a, j)"), "{f}");
    assert_eq!(accesses(&f), (0, 1), "{f}");
    let b = func_text(&m, "fill$b");
    assert!(b.contains("prim bytes_set_i64_unchecked(a, i, k)"), "{b}");
    assert!(b.contains("tailcall fill$b(n, a, j)"), "{b}");
    // Other tail calls stay.
    assert!(b.contains("tailcall fin(a)"), "{b}");
    assert_eq!(accesses(&b), (1, 0), "{b}");
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "9");
    // The same loop with other guards: `i < n` continuing in the default
    // arm, and stopping at `i == n` (then `i <= n` is needed too).
    for (guard, want) in [
        (
            "  let d = prim lt(i, n)\n  switch d { 0 => done, default => body }",
            vec!["i >= 0", "n <= words(a)"],
        ),
        (
            "  let d = prim eq(i, n)\n  switch d { 0 => body, default => done }",
            vec!["i <= n", "i >= 0", "n <= words(a)"],
        ),
        (
            "  let d = prim ge(i, n)\n  switch d { 1 => done, default => body }",
            vec!["i >= 0", "n <= words(a)"],
        ),
    ] {
        let src = src.replace(
            "  let d = prim ge(i, n)\n  switch d { 0 => body, default => done }",
            guard,
        );
        assert_eq!(precondition(&src, "fill"), want, "{guard}");
        let m = hoisted(&src, 1);
        assert_eq!(accesses(&func_text(&m, "fill$b")), (1, 0), "{guard}");
    }
    // The precondition holds, or fails on entry (more elements than the
    // array has, a negative start): the original checks report the same
    // panic as before.
    for (args, out) in [
        ("fill(2, a, 0)", Some("0")),
        ("fill(0, a, 7)", Some("0")),
        ("fill(5, a, 0)", None),
        ("fill(4, a, -1)", None),
    ] {
        let m = hoisted(&BOUNDS_FILL.replace("fill(N, a, I)", args), 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        match out {
            Some(o) => assert_eq!(r.stdout, o, "{args}"),
            None => assert_eq!(r.stderr, "panic: index out of bounds\n", "{args}"),
        }
    }
}

/// The partition loop of quicksort: `a[i]` is in bounds because `i <= j`
/// and `j < hi` on the path to it, and `hi < len(a)`.
const BOUNDS_PARTITION: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn part(hi: i64, x: i64, a: ptr @owned, i: i64, j: i64) -> (ptr, i64) {
entry:
  let d = prim ge(j, hi)
  switch d { 0 => body, default => done }
body:
  let v = prim bytes_get_i64(a, j)
  let l = prim lt(v, x)
  switch l { 0 => skip, default => swap }
skip:
  let j1 = prim add(j, 1)
  tailcall part(hi, x, a, i, j1)
swap:
  let ai = prim bytes_get_i64(a, i)
  let aj = prim bytes_get_i64(a, j)
  let s1 = prim bytes_set_i64(a, i, aj)
  let s2 = prim bytes_set_i64(a, j, ai)
  let i1 = prim add(i, 1)
  let j2 = prim add(j, 1)
  tailcall part(hi, x, a, i1, j2)
done:
  let bi = prim bytes_get_i64(a, i)
  let bh = prim bytes_get_i64(a, hi)
  let s3 = prim bytes_set_i64(a, i, bh)
  let s4 = prim bytes_set_i64(a, hi, bi)
  ret (a, i)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(64)
  let w0 = prim bytes_set_i64(a, 0, 5)
  let w1 = prim bytes_set_i64(a, 1, 2)
  let w2 = prim bytes_set_i64(a, 2, 8)
  let w3 = prim bytes_set_i64(a, 3, 1)
  let w4 = prim bytes_set_i64(a, 4, 9)
  let w5 = prim bytes_set_i64(a, 5, 3)
  let w6 = prim bytes_set_i64(a, 6, 7)
  let w7 = prim bytes_set_i64(a, 7, 4)
  let (b, k) = call part(HI, 4, a, LO, LO)
  let _ = extern cirka_write_i64(k)
  let r0 = prim bytes_get_i64(b, 0)
  let _ = extern cirka_write_i64(r0)
  let r1 = prim bytes_get_i64(b, 1)
  let _ = extern cirka_write_i64(r1)
  let r2 = prim bytes_get_i64(b, 2)
  let _ = extern cirka_write_i64(r2)
  let r3 = prim bytes_get_i64(b, 3)
  let _ = extern cirka_write_i64(r3)
  let r4 = prim bytes_get_i64(b, 4)
  let _ = extern cirka_write_i64(r4)
  let r5 = prim bytes_get_i64(b, 5)
  let _ = extern cirka_write_i64(r5)
  let r6 = prim bytes_get_i64(b, 6)
  let _ = extern cirka_write_i64(r6)
  let r7 = prim bytes_get_i64(b, 7)
  let _ = extern cirka_write_i64(r7)
  drop b
  ret 0
}
"#;

#[test]
fn bounds_checks_use_relations_of_the_loop_variables() {
    let src = BOUNDS_PARTITION.replace("HI", "7").replace("LO", "0");
    let m = parse(FileId(0), &src).unwrap();
    let pre = crate::bounds::precondition_of(m.func("part").unwrap()).unwrap();
    assert!(pre.contains(&"i <= j".to_string()), "{pre:?}");
    let m = hoisted(&src, 1);
    let b = func_text(&m, "part$b");
    assert_eq!(accesses(&b), (9, 0), "{b}");
    // Pivot 4 ends at index 3.
    assert_eq!(
        crate::interp::run(&m, b"", 1_000_000).unwrap().stdout,
        "321349875"
    );
    // `hi` past the end, a negative start.
    for (hi, lo) in [("8", "0"), ("7", "-1")] {
        let src = BOUNDS_PARTITION.replace("HI", hi).replace("LO", lo);
        let m = hoisted(&src, 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!(r.stderr, "panic: index out of bounds\n", "{hi} {lo}");
    }
}

/// The inner loop of n-body: fields at `7 * i + k` of five bodies, so the
/// array needs at least 35 elements.
const BOUNDS_BODIES: &str = r#"
extern fn cirka_write_f64(f64) -> void

fn mv(a: ptr @owned, i: i64) -> ptr {
entry:
  let d = prim ge(i, 5)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let b = prim mul(i, 7)
  let x = prim bytes_get_f64(a, b)
  let k = prim add(b, 6)
  let v = prim bytes_get_f64(a, k)
  let y = prim fadd(x, v)
  let s = prim bytes_set_f64(a, b, y)
  let j = prim add(i, 1)
  tailcall mv(a, j)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(BYTES)
  let w = prim bytes_set_f64(a, 27, 1.5)
  let b = call mv(a, 0)
  let r = prim bytes_get_f64(b, 21)
  drop b
  let _ = extern cirka_write_f64(r)
  ret 0
}
"#;

#[test]
fn bounds_checks_use_constant_lengths() {
    let src = BOUNDS_BODIES.replace("BYTES", "280");
    let m = parse(FileId(0), &src).unwrap();
    assert_eq!(
        crate::bounds::precondition_of(m.func("mv").unwrap()),
        Some(vec!["i >= 0".to_string(), "words(a) >= 35".to_string()])
    );
    let m = hoisted(&src, 1);
    assert_eq!(accesses(&func_text(&m, "mv$b")), (3, 0));
    assert_eq!(
        crate::interp::run(&m, b"", 1_000_000).unwrap().stdout,
        "1.5"
    );
    // 34 elements: the last body reads past the end.
    let m = hoisted(&BOUNDS_BODIES.replace("BYTES", "272"), 1);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // Inside `i < 6`, `2 * i < 10` bounds `i` by `9 / 2`, rounded down, so
    // `a[i + 4]` needs 9 elements.
    let scaled = r#"
extern fn cirka_write_i64(i64) -> void
fn sc(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, 6)
  switch d { 0 => body, default => done }
done:
  ret acc
body:
  let e = prim mul(i, 2)
  let f = prim lt(e, 10)
  switch f { 0 => skip, default => read }
read:
  let k = prim add(i, 4)
  let x = prim bytes_get_i64(a, k)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall sc(a, j, acc1)
skip:
  let j2 = prim add(i, 1)
  tailcall sc(a, j2, acc)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(BYTES)
  let w = prim bytes_set_i64(a, 4, 3)
  let r = call sc(a, 0, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let src = scaled.replace("BYTES", "72");
    assert_eq!(precondition(&src, "sc"), vec!["i >= 0", "words(a) >= 9"]);
    let m = hoisted(&src, 1);
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "3");
    let m = hoisted(&scaled.replace("BYTES", "64"), 1);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // Constant indices: the passed check of `a[5]` does not prove `a[6]`.
    let fixed = r#"
extern fn cirka_write_i64(i64) -> void
fn two(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, 3)
  switch d { 0 => body, default => done }
done:
  ret acc
body:
  let x = prim bytes_get_i64(a, 5)
  let y = prim bytes_get_i64(a, 6)
  let s = prim add(x, y)
  let acc1 = prim add(acc, s)
  let j = prim add(i, 1)
  tailcall two(a, j, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(BYTES)
  let w = prim bytes_set_i64(a, 5, 2)
  let r = call two(a, 0, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let src = fixed.replace("BYTES", "56");
    assert_eq!(precondition(&src, "two"), vec!["words(a) >= 7"]);
    let m = hoisted(&src, 1);
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "6");
    let m = hoisted(&fixed.replace("BYTES", "48"), 1);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
}

const BOUNDS_REVERSE: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn rev(a: ptr @owned, lo: i64, hi: i64) -> ptr {
entry:
  let d = prim ge(lo, hi)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let x = prim bytes_get_i64(a, lo)
  let y = prim bytes_get_i64(a, hi)
  let s = prim bytes_set_i64(a, lo, y)
  let t = prim bytes_set_i64(a, hi, x)
  let lo1 = prim add(lo, 1)
  let hi1 = prim sub(hi, 1)
  tailcall rev(a, lo1, hi1)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 1, 7)
  let b = call rev(a, LO, HI)
  let r = prim bytes_get_i64(b, 2)
  drop b
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;

#[test]
fn bounds_checks_of_loops_from_both_ends() {
    let src = BOUNDS_REVERSE.replace("LO", "0").replace("HI", "3");
    let m = parse(FileId(0), &src).unwrap();
    assert_eq!(
        crate::bounds::precondition_of(m.func("rev").unwrap()),
        Some(vec!["lo >= 0".to_string(), "hi < words(a)".to_string()])
    );
    let m = hoisted(&src, 1);
    assert_eq!(accesses(&func_text(&m, "rev$b")), (4, 0));
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "7");
    for (lo, hi) in [("0", "4"), ("-1", "3")] {
        let m = hoisted(&BOUNDS_REVERSE.replace("LO", lo).replace("HI", hi), 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!(r.stderr, "panic: index out of bounds\n", "{lo} {hi}");
    }
    // Counting down to 0: the guard `i >= 0` gives the lower bound.
    let down = r#"
extern fn cirka_write_i64(i64) -> void
fn down(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, 0)
  switch d { 0 => done, default => body }
done:
  ret acc
body:
  let x = prim bytes_get_i64(a, i)
  let acc1 = prim add(acc, x)
  let j = prim sub(i, 1)
  tailcall down(a, j, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 0, 4)
  let r = call down(a, START, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let src = down.replace("START", "3");
    assert_eq!(precondition(&src, "down"), vec!["i < words(a)"]);
    let m = hoisted(&src, 1);
    assert_eq!(accesses(&func_text(&m, "down$b")), (1, 0));
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "4");
    let m = hoisted(&down.replace("START", "4"), 1);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
}

#[test]
fn bounds_checks_without_a_proof_stay() {
    // An index read from the array: only the first access goes.
    let gather = r#"
extern fn cirka_write_i64(i64) -> void
fn gather(a: ptr @borrowed, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret acc
body:
  let k = prim bytes_get_i64(a, i)
  let x = prim bytes_get_i64(a, k)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall gather(a, j, n, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let w0 = prim bytes_set_i64(a, 0, 1)
  let w1 = prim bytes_set_i64(a, 1, 2)
  let w2 = prim bytes_set_i64(a, 2, K)
  let w3 = prim bytes_set_i64(a, 3, 9)
  let r = call gather(a, 0, 3, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let m = hoisted(&gather.replace("K", "3"), 1);
    assert_eq!(accesses(&func_text(&m, "gather$b")), (1, 1));
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "14");
    let m = hoisted(&gather.replace("K", "7"), 1);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // The same with an unnamed store (`let _ = ...`).
    let scatter = r#"
extern fn cirka_write_i64(i64) -> void
fn scatter(a: ptr @borrowed, i: i64, n: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret 0
body:
  let k = prim bytes_get_i64(a, i)
  let _ = prim bytes_set_i64(a, k, i)
  let j = prim add(i, 1)
  tailcall scatter(a, j, n)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 0, K)
  let r = call scatter(a, 0, 2)
  let x = prim bytes_get_i64(a, 0)
  drop a
  let _ = extern cirka_write_i64(x)
  ret 0
}
"#;
    let m = hoisted(&scatter.replace("K", "1"), 1);
    assert_eq!(accesses(&func_text(&m, "scatter$b")), (1, 1));
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "1");
    let m = hoisted(&scatter.replace("K", "9"), 1);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // The next iteration reads the other array: nothing is known about it.
    let swap = r#"
extern fn cirka_write_i64(i64) -> void
fn alt(a: ptr @borrowed, b: ptr @borrowed, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret acc
body:
  let x = prim bytes_get_i64(a, i)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall alt(b, a, j, n, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let b = prim bytes_new(16)
  let r = call alt(a, b, 0, 4, 0)
  drop a
  drop b
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let m = hoisted(swap, 0);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // A fresh array replaces the parameter: its length is unknown.
    let shrink = r#"
extern fn cirka_write_i64(i64) -> void
fn shrink(a: ptr @owned, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop a
  ret acc
body:
  let x = prim bytes_get_i64(a, i)
  let acc1 = prim add(acc, x)
  drop a
  let c = prim bytes_new(16)
  let j = prim add(i, 1)
  tailcall shrink(c, j, n, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let r = call shrink(a, 0, N, 0)
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let m = hoisted(&shrink.replace("N", "2"), 0);
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "0");
    let m = hoisted(&shrink.replace("N", "3"), 0);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // A quarter of the length in bytes is not the number of words.
    let quarter = r#"
extern fn cirka_write_i64(i64) -> void
fn quarter(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let b = prim bytes_len(a)
  let n = prim lshr(b, 2)
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret acc
body:
  let x = prim bytes_get_i64(a, i)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall quarter(a, j, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let r = call quarter(a, 0, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let m = hoisted(quarter, 0);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // A negative factor (`7 - i`) is not followed.
    let back = r#"
extern fn cirka_write_i64(i64) -> void
fn back(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, 8)
  switch d { 0 => body, default => done }
done:
  ret acc
body:
  let m = prim mul(i, -1)
  let k = prim add(m, 7)
  let x = prim bytes_get_i64(a, k)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall back(a, j, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(BYTES)
  let w = prim bytes_set_i64(a, 0, 3)
  let r = call back(a, 0, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let m = hoisted(&back.replace("BYTES", "64"), 0);
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "3");
    let m = hoisted(&back.replace("BYTES", "32"), 0);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stderr, "panic: index out of bounds\n");
    // `i + 1` wraps around for `i = i64::MAX` (then `k < 8` holds): the
    // guard bounds `i` only when the addition is known not to wrap.
    let wrap = r#"
extern fn cirka_write_i64(i64) -> void
fn wrap(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let k = prim add(i, 1)
  let d = prim lt(k, 8)
  switch d { 0 => done, default => body }
done:
  ret acc
body:
  let x = prim bytes_get_i64(a, i)
  let acc1 = prim add(acc, x)
  tailcall wrap(a, k, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(64)
  let w = prim bytes_set_i64(a, 6, 5)
  let r = call wrap(a, I, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    // So does a product: `i * 2^32` is 0 again for `i = 2^32`.
    let wrap_mul = wrap
        .replace("prim add(i, 1)", "prim mul(i, 4294967296)")
        .replace(
            "  tailcall wrap(a, k, acc1)",
            "  let j = prim add(i, 1)\n  tailcall wrap(a, j, acc1)",
        );
    for (src, i, out) in [
        (wrap, "0", "5"),
        (wrap, "9223372036854775807", ""),
        (&wrap_mul, "0", "0"),
        (&wrap_mul, "4294967296", ""),
        (&wrap_mul, "-1", ""),
    ] {
        let src = src.replace("I,", &format!("{i},"));
        let before = parse(FileId(0), &src).unwrap();
        let mut m = before.clone();
        crate::bounds::hoist_checks(&mut m);
        let r0 = crate::interp::run(&before, b"", 1_000_000).unwrap();
        let r1 = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!(r1.stdout, out);
        assert_eq!(
            (r0.stdout, r0.stderr, r0.exit_code),
            (r1.stdout, r1.stderr, r1.exit_code)
        );
    }
}

#[test]
fn unchecked_accesses_round_trip_and_fault_out_of_bounds() {
    let unchecked: Vec<Prim> = Prim::ALL
        .iter()
        .copied()
        .filter(|p| p.is_unchecked())
        .collect();
    assert_eq!(unchecked.len(), 11);
    for p in unchecked {
        assert_eq!(p.checked().unchecked(), Some(p));
        assert_eq!(Prim::from_name(p.name()), Some(p));
        assert_eq!(p.signature(), p.checked().signature());
    }
    let src = "extern fn cirka_write_i64(i64) -> void
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let s = prim bytes_set_i64_unchecked(a, 3, 6)
  let x = prim bytes_get_i64_unchecked(a, IDX)
  drop a
  let _ = extern cirka_write_i64(x)
  ret 0
}";
    let ok = src.replace("IDX", "3");
    let m = parse(FileId(0), &ok).unwrap();
    let again = parse(FileId(0), &print_module(&m)).unwrap();
    assert_eq!(strip_spans(m.clone()), strip_spans(again));
    assert!(crate::verify::verify_module(&m).is_empty());
    assert_eq!(interp(&ok).unwrap().stdout, "6");
    // Out of bounds, an unchecked access is a compiler bug, not a panic.
    let e = interp(&src.replace("IDX", "4")).unwrap_err();
    assert!(
        e.contains("bytes_get_i64_unchecked: unchecked access failed: index out of bounds"),
        "{e}"
    );
}

#[test]
fn bounds_checks_of_loops_over_the_whole_array() {
    // `bytes_len / 8` and `array_len` bound the index: only `i >= 0` is
    // needed.
    let words = r#"
extern fn cirka_write_i64(i64) -> void
fn sum(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let b = prim bytes_len(a)
  let n = prim lshr(b, 3)
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret acc
body:
  let x = prim bytes_get_i64(a, i)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall sum(a, j, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(36)
  let w = prim bytes_set_i64(a, 3, 8)
  let r = call sum(a, START, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let elems = r#"
type Int { I = 0 { v: i64 } }
extern fn cirka_write_i64(i64) -> void
fn fill(a: ptr @borrowed, i: i64) -> i64 {
entry:
  let n = prim array_len(a)
  let c = prim ge(i, n)
  switch c { 1 => done, default => more }
done:
  ret 0
more:
  let sq = prim mul(i, i)
  let b = alloc Int.I(sq)
  let _ = prim array_set(a, i, b)
  let i1 = prim add(i, 1)
  tailcall fill(a, i1)
}
fn main() -> i64 {
entry:
  let z = alloc Int.I(0)
  let a = prim array_new(10, z)
  let _ = call fill(a, START)
  let e = prim array_get(a, 7)
  let v = proj e Int.I.v
  let _ = extern cirka_write_i64(v)
  drop a
  ret 0
}
"#;
    for (src, name, out) in [(words, "sum", "8"), (elems, "fill", "49")] {
        let ok = src.replace("START", "0");
        assert_eq!(precondition(&ok, name), vec!["i >= 0"], "{name}");
        let m = hoisted(&ok, 1);
        assert_eq!(accesses(&func_text(&m, &format!("{name}$b"))), (1, 0));
        assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, out);
        let m = hoisted(&src.replace("START", "-1"), 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!(r.stderr, "panic: index out of bounds\n", "{name}");
    }
}

/// A loop over an array parameter that may be a thunk: each iteration
/// forces it, and passes the value to the next.
const BOUNDS_LAZY: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 2, 7)
  ret a
}

fn boom() -> ptr {
entry:
  panic "boom"
}

fn sum(a: ptr @owned, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop a
  ret acc
body:
  let ev = force a
  let x = prim bytes_get_i64(ev, i)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall sum(ev, j, n, acc1)
}

fn main() -> i64 {
entry:
  let t = ARRAY
  FORCED
  let r = call sum(t, START, N, 0)
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;

#[test]
fn bounds_checks_of_lazy_arrays() {
    let lazy = |array: &str, start: &str, n: &str| {
        BOUNDS_LAZY
            .replace("ARRAY", array)
            .replace("START", start)
            .replace("N,", &format!("{n},"))
            .replace("  FORCED\n", "")
    };
    let src = lazy("mkthunk mk()", "0", "4");
    assert_eq!(precondition(&src, "sum"), vec!["i >= 0", "n <= words(a)"]);
    let m = hoisted(&src, 1);
    // The length is read only from a value: a thunk enters the checked
    // loop, whose first iteration forces it, and the next iteration the
    // unchecked one.
    let f = func_text(&m, "sum");
    assert!(f.contains("prim is_value(a)"), "{f}");
    assert!(f.contains("tailcall sum$b(a, i, n, acc)"), "{f}");
    assert_eq!(accesses(&func_text(&m, "sum$b")), (1, 0));
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "7");
    // An array evaluated before the call; past the end; a thunk that
    // panics, which the entry test must not force when the loop does not.
    for (array, start, n, out) in [
        ("prim bytes_new(32)", "0", "4", Some("0")),
        ("mkthunk mk()", "0", "5", None),
        ("mkthunk boom()", "3", "3", Some("0")),
        ("mkthunk boom()", "0", "3", None),
    ] {
        let m = hoisted(&lazy(array, start, n), 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        match out {
            Some(o) => assert_eq!(r.stdout, o, "{array} {start} {n}"),
            None => assert!(r.stderr.starts_with("panic: "), "{array}: {}", r.stderr),
        }
    }
    // A thunk forced before the call is an indirection to the array, not
    // the array: the first iteration runs checked.
    let src = BOUNDS_LAZY
        .replace("ARRAY", "mkthunk mk()")
        .replace("START", "0")
        .replace("N,", "4,")
        .replace("  FORCED\n", "  dup t\n  let v = force t\n  drop v\n");
    let m = hoisted(&src, 1);
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "7");
}

#[test]
fn bounds_checks_proved_by_the_guards_go_in_place() {
    // `0 <= i < len` is tested by the loop itself: no precondition, no
    // copy.
    let src = r#"
extern fn cirka_write_i64(i64) -> void
fn safe(a: ptr @borrowed, i: i64, acc: i64) -> i64 {
entry:
  let n = prim bytes_len(a)
  let w = prim lshr(n, 3)
  let d = prim lt(i, w)
  switch d { 0 => done, default => pos }
pos:
  let e = prim ge(i, 0)
  switch e { 0 => done, default => body }
done:
  ret acc
body:
  let x = prim bytes_get_i64(a, i)
  let y = prim bytes_get_i64(a, x)
  let acc1 = prim add(acc, y)
  let j = prim add(i, 1)
  tailcall safe(a, j, acc1)
}
fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 3, K)
  let v = prim bytes_set_i64(a, 2, 1)
  let r = call safe(a, START, 0)
  drop a
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    // `a[x]` with `x` read from the array keeps its check.
    for (start, k, out) in [
        ("0", "2", Some("1")),
        ("-1", "2", Some("0")),
        ("0", "9", None),
    ] {
        let src = src.replace("START", start).replace("K)", &format!("{k})"));
        assert_eq!(precondition(&src, "safe"), Vec::<String>::new());
        let m = hoisted(&src, 1);
        assert!(m.func("safe$b").is_none());
        let f = func_text(&m, "safe");
        assert_eq!(accesses(&f), (1, 1), "{f}");
        assert!(!f.contains("tailcall safe$b"), "{f}");
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        match out {
            Some(o) => assert_eq!(r.stdout, o, "{start} {k}"),
            None => assert_eq!(r.stderr, "panic: index out of bounds\n"),
        }
    }
}

// ---- FEAT-0070: loops entered through the unique specialization ----

/// The loop passes on `src` in the order of `perceus::run`. Checks that the
/// result passes the verifier and runs like the original in the
/// interpreter, which traps a store into a shared array and an access to a
/// thunk (a wrong entry test). Returns the transformed module.
fn entered(src: &str, specialized: usize) -> Module {
    let before = parse(FileId(0), src).unwrap();
    let mut m = before.clone();
    let done = crate::uniq::specialize_loops(&mut m);
    assert_eq!(done.len(), specialized);
    crate::bounds::hoist_checks(&mut m);
    crate::uniq::enter_specialized(&mut m, &done);
    let text = print_module(&m);
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let r0 = crate::interp::run(&before, b"", 1_000_000).unwrap();
    let r1 = crate::interp::run(&m, b"", 1_000_000).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(r0.stdout, r1.stdout, "{text}");
    assert_eq!(r0.stderr, r1.stderr, "{text}");
    assert_eq!(r0.exit_code, r1.exit_code, "{text}");
    assert_eq!(r0.stats.allocs, r1.stats.allocs, "{text}");
    assert_eq!(r0.leaked, r1.leaked, "{text}");
    m
}

/// The functions other than `f` itself and the specialized loop that tail
/// call the specialized loop of `f`.
fn hot_callers(m: &Module, f: &str) -> Vec<String> {
    let hot = [format!("{f}$u"), format!("{f}$u$b")];
    m.funcs
        .iter()
        .filter(|g| g.name != f && !hot.contains(&g.name))
        .filter(|g| {
            g.blocks
                .iter()
                .any(|b| matches!(&b.term, Terminator::TailCall { func, .. } if hot.contains(func)))
        })
        .map(|g| g.name.clone())
        .collect()
}

const UNIQUE_ENTRY: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn fill(a: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let c = prim bytes_unique(a)
  let w = prim bytes_set_i64(c, i, 9)
  let j = prim add(i, 1)
  tailcall fill(c, j, n)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  SHARE
  let b = call fill(a, 0, 4)
  let x = prim bytes_get_i64(b, 3)
  USE
  drop b
  let _ = extern cirka_write_i64(x)
  ret 0
}
"#;

/// Text that reads `name[index]`, drops it and prints the element.
fn print_and_drop(name: &str, index: i64) -> String {
    format!(
        "  let y = prim bytes_get_i64({name}, {index})\n  drop {name}\n  let _ = extern cirka_write_i64(y)\n"
    )
}

#[test]
fn unique_entry_goes_to_the_specialized_loop() {
    let src = UNIQUE_ENTRY.replace("  SHARE\n", "").replace("  USE\n", "");
    let m = entered(&src, 1);
    let f = func_text(&m, "fill");
    // The array is tested, not forced: nothing in `fill` forces it.
    assert!(f.contains("prim is_unique(a)"), "{f}");
    assert!(!f.contains("is_value"), "{f}");
    assert!(f.contains("tailcall fill$u(a, i, n)"), "{f}");
    assert!(f.contains("tailcall fill$slow(a, i, n)"), "{f}");
    assert!(!f.contains("bytes_unique"), "{f}");
    // The first iteration of a shared array runs in the cold body.
    let slow = func_text(&m, "fill$slow");
    assert!(slow.contains("cold fn fill$slow("), "{slow}");
    assert!(slow.contains("prim bytes_unique(a)"), "{slow}");
    assert!(!m.func("fill").unwrap().cold);
    // After the check the cold path goes on in copies of its own, optimized
    // like any other function: a loop that starts with a shared array spends
    // its time there.
    assert!(slow.contains("tailcall fill$slow$u(c, j, n)"), "{slow}");
    for copy in ["fill$slow$u", "fill$slow$u$b"] {
        assert!(!m.func(copy).unwrap().cold, "{copy}");
    }
    // Every iteration of the old body checks: no test is repeated.
    assert!(m.func("fill$again").is_none());
    assert!(!m.func("fill$u").unwrap().cold);
    assert!(!m.func("fill$u$b").unwrap().cold);
    assert!(
        hot_callers(&m, "fill").is_empty(),
        "{:?}",
        hot_callers(&m, "fill")
    );
    // `fill` has no self tail call left, so only the specialized loop gets
    // a copy without the bounds checks.
    assert!(m.func("fill$b").is_none());
    assert!(m.func("fill$u$b").is_some());
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stdout, "9");
    assert_eq!(r.stats.allocs, 1);
    assert_eq!(r.leaked, 0);
    // The specialized function is not given an entry test of its own.
    assert!(!func_text(&m, "fill$u").contains("is_unique"));
}

#[test]
fn unique_entry_copies_a_shared_array_as_before() {
    let src = UNIQUE_ENTRY
        .replace("  SHARE\n", "  dup a\n")
        .replace("  USE\n", &print_and_drop("a", 3));
    let m = entered(&src, 1);
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    // The caller's array keeps its zero; the loop writes into one copy.
    assert_eq!(r.stdout, "09");
    assert_eq!(r.stats.allocs, 2);
    assert_eq!(r.leaked, 0);
}

const UNIQUE_ENTRY_LAZY: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 2, 7)
  ret a
}

fn boom() -> ptr {
entry:
  panic "boom"
}

fn sum(a: ptr @owned, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop a
  ret acc
body:
  let ev = force a
  let c = prim bytes_unique(ev)
  let x = prim bytes_get_i64(c, i)
  let w = prim bytes_set_i64(c, i, 0)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall sum(c, j, n, acc1)
}

fn main() -> i64 {
entry:
  let t = ARRAY
  BEFORE
  let r = call sum(t, START, COUNT, 0)
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;

#[test]
fn unique_entry_tests_lazy_arrays_without_forcing() {
    let lazy = |array: &str, before: &str, start: &str, count: &str| {
        UNIQUE_ENTRY_LAZY
            .replace("ARRAY", array)
            .replace("  BEFORE\n", before)
            .replace("START", start)
            .replace("COUNT", count)
    };
    let m = entered(&lazy("mkthunk mk()", "", "0", "4"), 1);
    // A forced parameter must be a value for the specialized function, which
    // has no `force` left.
    let f = func_text(&m, "sum");
    assert!(f.contains("prim is_value(a)"), "{f}");
    assert!(f.contains("prim is_unique(a)"), "{f}");
    assert!(f.contains("tailcall sum$u(a, i, n, acc)"), "{f}");
    assert!(!func_text(&m, "sum$u").contains("force"));
    let evaluated = "  dup t\n  let v = force t\n  drop v\n";
    for (array, before, start, count, out) in [
        // A value, a thunk, and a thunk already evaluated (an indirection).
        ("prim bytes_new(32)", "", "0", "4", Some("0")),
        ("mkthunk mk()", "", "0", "4", Some("7")),
        ("mkthunk mk()", evaluated, "0", "4", Some("7")),
        // Past the end, after the thunk is evaluated.
        ("mkthunk mk()", "", "0", "5", None),
        // A thunk that panics is forced only if the loop runs.
        ("mkthunk boom()", "", "3", "3", Some("0")),
        ("mkthunk boom()", "", "0", "3", None),
    ] {
        let m = entered(&lazy(array, before, start, count), 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        match out {
            Some(o) => assert_eq!(r.stdout, o, "{array} {before} {start} {count}"),
            None => assert!(r.stderr.starts_with("panic: "), "{array}: {}", r.stderr),
        }
    }
}

const UNIQUE_ENTRY_TWO: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn both(a: ptr @owned, b: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop b
  ret a
body:
  let ca = prim bytes_unique(a)
  let cb = prim bytes_unique(b)
  let w1 = prim bytes_set_i64(ca, i, 1)
  let w2 = prim bytes_set_i64(cb, i, 2)
  let j = prim add(i, 1)
  tailcall both(ca, cb, j, n)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(16)
  let b = prim bytes_new(16)
  SHARE
  let r = call both(a, b, 0, 2)
  let x = prim bytes_get_i64(r, 1)
  USE
  drop r
  let _ = extern cirka_write_i64(x)
  ret 0
}
"#;

#[test]
fn unique_entry_tests_every_assumed_array() {
    let m = entered(
        &UNIQUE_ENTRY_TWO
            .replace("  SHARE\n", "")
            .replace("  USE\n", ""),
        1,
    );
    let f = func_text(&m, "both");
    assert!(f.contains("prim is_unique(a)"), "{f}");
    assert!(f.contains("prim is_unique(b)"), "{f}");
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stdout, "1");
    assert_eq!(r.stats.allocs, 2);
    // Either array shared: the shared one is copied and keeps its zero.
    for shared in ["a", "b"] {
        let src = UNIQUE_ENTRY_TWO
            .replace("  SHARE\n", &format!("  dup {shared}\n"))
            .replace("  USE\n", &print_and_drop(shared, 1));
        let m = entered(&src, 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!(r.stdout, "01", "{shared}");
        assert_eq!(r.stats.allocs, 3, "{shared}");
        assert_eq!(r.leaked, 0, "{shared}");
    }
}

#[test]
fn unique_entry_tests_only_the_assumed_arrays() {
    // `src` is read and never checked, so the loop is entered whatever its
    // reference count is: it stays shared with the caller and is not copied.
    let src = r#"
extern fn cirka_write_i64(i64) -> void

fn copy(src: ptr @owned, dst: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop src
  ret dst
body:
  let x = prim bytes_get_i64(src, i)
  let c = prim bytes_unique(dst)
  let w = prim bytes_set_i64(c, i, x)
  let j = prim add(i, 1)
  tailcall copy(src, c, j, n)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(16)
  let w = prim bytes_set_i64(a, 1, 5)
  let b = prim bytes_new(16)
  dup a
  let r = call copy(a, b, 0, 2)
  let x = prim bytes_get_i64(r, 1)
  let y = prim bytes_get_i64(a, 1)
  drop r
  drop a
  let _ = extern cirka_write_i64(x)
  let _ = extern cirka_write_i64(y)
  ret 0
}
"#;
    let m = entered(src, 1);
    let f = func_text(&m, "copy");
    assert_eq!(f.matches("prim is_unique").count(), 1, "{f}");
    assert!(f.contains("prim is_unique(dst)"), "{f}");
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.stdout, "55");
    assert_eq!(r.stats.allocs, 2);
}

#[test]
fn unique_entry_keeps_loops_that_share_their_arrays() {
    // The programs of FEAT-0067: the array is shared before the first write
    // and in the middle of the loop.
    assert!(hot_callers(&entered(UNIQUE_LOOP, 1), "fill").is_empty());
    let m = entered(UNIQUE_ROUTES, 1);
    assert_eq!(
        crate::interp::run(&m, b"", 1_000_000).unwrap().stdout,
        "402"
    );
    // A loop that leaves the specialized function comes back to the test.
    assert!(func_text(&m, "run$u").contains("tailcall run(a, a, j, n)"));
    // Its copy stays on the cold path.
    let copy = func_text(&m, "run$slow$u");
    assert!(copy.contains("tailcall run$slow(a, a, j, n)"), "{copy}");
    assert!(
        hot_callers(&m, "run").is_empty(),
        "{:?}",
        hot_callers(&m, "run")
    );
}

#[test]
fn unique_entry_keeps_the_body_without_checks_on_the_cold_path() {
    // A read at `i` makes the bounds checks of `mark` worth hoisting, and
    // the path that does not write keeps a self tail call in the old body.
    let src = r#"
extern fn cirka_write_i64(i64) -> void

fn mark(a: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let v = prim bytes_get_i64(a, i)
  let j = prim add(i, 1)
  switch v { 0 => write, default => skip }
skip:
  tailcall mark(a, j, n)
write:
  let c = prim bytes_unique(a)
  let w = prim bytes_set_i64(c, i, 7)
  tailcall mark(c, j, n)
}

fn main() -> i64 {
entry:
  let a = prim bytes_new(32)
  dup a
  let b = call mark(a, 0, 4)
  let x = prim bytes_get_i64(b, 3)
  let y = prim bytes_get_i64(a, 3)
  drop a
  drop b
  let _ = extern cirka_write_i64(x)
  let _ = extern cirka_write_i64(y)
  ret 0
}
"#;
    let m = entered(src, 1);
    assert!(m.func("mark$b").unwrap().cold);
    let b = func_text(&m, "mark$b");
    assert!(b.contains("tailcall mark$b(a, j, n)"), "{b}");
    assert!(b.contains("tailcall mark$slow$u(c, j, n)"), "{b}");
    let copy = func_text(&m, "mark$slow$u$b");
    assert!(!m.func("mark$slow$u$b").unwrap().cold);
    assert!(copy.contains("tailcall mark$slow$u$b(a, j, n)"), "{copy}");
    assert!(
        hot_callers(&m, "mark").is_empty(),
        "{:?}",
        hot_callers(&m, "mark")
    );
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "70");
}

/// A loop that forces an array and only reads it.
const VALUE_LOOP: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 2, 7)
  ret a
}

fn sum(a: ptr @owned, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop a
  ret acc
body:
  let ev = force a
  let x = prim bytes_get_i64(ev, i)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall sum(ev, j, n, acc1)
}

fn main() -> i64 {
entry:
  let t = ARRAY
  BEFORE
  let r = call sum(t, 0, 4, 0)
  AFTER
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;

fn value_loop(array: &str, before: &str, after: &str) -> String {
    VALUE_LOOP
        .replace("ARRAY", array)
        .replace("  BEFORE\n", before)
        .replace("  AFTER\n", after)
}

#[test]
fn value_loops_test_that_their_arrays_are_values() {
    let m = entered(&value_loop("mkthunk mk()", "", ""), 1);
    // Only the value-ness matters: the array is read, never written.
    let f = func_text(&m, "sum");
    assert!(f.contains("prim is_value(a)"), "{f}");
    assert!(!f.contains("is_unique"), "{f}");
    assert!(f.contains("tailcall sum$u(a, i, n, acc)"), "{f}");
    assert!(f.contains("tailcall sum$slow(a, i, n, acc)"), "{f}");
    assert!(!func_text(&m, "sum$u").contains("force"));
    assert!(func_text(&m, "sum$slow").contains("force a"));
    assert!(
        hot_callers(&m, "sum").is_empty(),
        "{:?}",
        hot_callers(&m, "sum")
    );
    let evaluated = "  dup t\n  let v = force t\n  drop v\n";
    let shared_read = "  let y = force t\n  drop y\n";
    for (array, before, after, out) in [
        // A value, a thunk, and a thunk already evaluated (an indirection).
        ("prim bytes_new(32)", "", "", "0"),
        ("mkthunk mk()", "", "", "7"),
        ("mkthunk mk()", evaluated, "", "7"),
        // Shared: the loop reads it, the other owner keeps it.
        ("mkthunk mk()", "  dup t\n", shared_read, "7"),
    ] {
        let m = entered(&value_loop(array, before, after), 1);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!(r.stdout, out, "{array}");
        assert_eq!(r.leaked, 0, "{array}");
    }
}

#[test]
fn value_loops_go_on_after_a_shared_or_lazy_entry() {
    // The first iteration forces the array in the old body, and the next
    // ones run in the copy without `force` of that body.
    let m = entered(&value_loop("mkthunk mk()", "", ""), 1);
    let slow = func_text(&m, "sum$slow");
    assert!(
        slow.contains("tailcall sum$slow$u(ev, j, n, acc1)"),
        "{slow}"
    );
    assert!(!func_text(&m, "sum$slow$u").contains("force"));
}

#[test]
fn value_and_unique_parameters_are_assumed_together() {
    let src = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(16)
  let w = prim bytes_set_i64(a, 1, 5)
  ret a
}

fn copy(src: ptr @owned, dst: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop src
  ret dst
body:
  let ev = force src
  let x = prim bytes_get_i64(ev, i)
  let c = prim bytes_unique(dst)
  let w = prim bytes_set_i64(c, i, x)
  let j = prim add(i, 1)
  tailcall copy(ev, c, j, n)
}

fn main() -> i64 {
entry:
  let s = mkthunk mk()
  let d = prim bytes_new(16)
  let r = call copy(s, d, 0, 2)
  let y = prim bytes_get_i64(r, 1)
  drop r
  let _ = extern cirka_write_i64(y)
  ret 0
}
"#;
    let m = entered(src, 1);
    let f = func_text(&m, "copy");
    assert!(f.contains("prim is_value(src)"), "{f}");
    assert!(f.contains("prim is_unique(dst)"), "{f}");
    assert!(!f.contains("prim is_unique(src)"), "{f}");
    assert!(!f.contains("prim is_value(dst)"), "{f}");
    let u = func_text(&m, "copy$u");
    assert!(!u.contains("force"), "{u}");
    assert!(!u.contains("bytes_unique"), "{u}");
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "5");
}

/// A loop that forces the array only on some iterations. `SKIP` is the
/// argument the iterations that do not force pass on.
const VALUE_LOOP_PATHS: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 2, 7)
  ret a
}

fn sum(a: ptr @owned, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop a
  ret acc
body:
  let j = prim add(i, 1)
  let e = prim eq(i, 0)
  switch e { 0 => read, default => skip }
skip:
  tailcall sum(SKIP, j, n, acc)
read:
  let ev = force a
  let x = prim bytes_get_i64(ev, i)
  let acc1 = prim add(acc, x)
  tailcall sum(ev, j, n, acc1)
}

fn main() -> i64 {
entry:
  let t = mkthunk mk()
  let r = call sum(t, 0, 4, 0)
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;

#[test]
fn value_loops_enter_the_copy_only_after_a_force() {
    let m = entered(&VALUE_LOOP_PATHS.replace("SKIP", "a"), 1);
    // In the old body the array may still be a thunk on the path that does
    // not force it.
    let slow = func_text(&m, "sum$slow");
    assert!(slow.contains("tailcall sum$slow(a, j, n, acc)"), "{slow}");
    assert!(
        slow.contains("tailcall sum$slow$u(ev, j, n, acc1)"),
        "{slow}"
    );
    // The copy assumes a value, on both paths.
    let u = func_text(&m, "sum$u");
    assert!(u.contains("tailcall sum$u(a, j, n, acc)"), "{u}");
    assert!(u.contains("tailcall sum$u(a, j, n, acc1)"), "{u}");
    assert!(!u.contains("force"), "{u}");
}

#[test]
fn value_loops_need_the_forced_array_passed_on() {
    // The forced array is not what the next iteration gets.
    let src = VALUE_LOOP.replace(
        "tailcall sum(ev, j, n, acc1)",
        "drop ev\n  let o = prim bytes_new(32)\n  tailcall sum(o, j, n, acc1)",
    );
    let src = value_loop_with(&src, "prim bytes_new(32)");
    let m = entered(&src, 0);
    assert!(m.func("sum$u").is_none());
}

fn value_loop_with(src: &str, array: &str) -> String {
    src.replace("ARRAY", array)
        .replace("  BEFORE\n", "")
        .replace("  AFTER\n", "")
}

// ---- FEAT-0076: facts that cross calls of functions returning their array ----

/// What `kept_results` finds for `f` in the module `src`.
fn kept(src: &str, f: &str) -> Vec<Option<usize>> {
    crate::uniq::kept_results(&parse(FileId(0), src).unwrap())[f].clone()
}

/// What is kept by `f(a, b, i) -> ptr` with the given body.
fn kept_by(body: &str) -> Vec<Option<usize>> {
    kept(
        &format!(
            "type Box {{ B = 0 {{ v: ptr }} }}\n\
             fn f(a: ptr @owned, b: ptr @owned, i: i64) -> ptr {{\nentry:\n{body}\n}}\n"
        ),
        "f",
    )
}

#[test]
fn kept_results_are_parameters_used_only_in_place() {
    for (body, want) in [
        ("  drop b\n  ret a", Some(0)),
        ("  drop a\n  ret b", Some(1)),
        (
            "  drop b\n  let e = force a\n  let c = prim bytes_unique(e)\n  \
             let w = prim bytes_set_i64(c, i, 1)\n  let x = prim bytes_get_i64(c, i)\n  ret c",
            Some(0),
        ),
        // A copy of it, stored somewhere, or another array is returned.
        ("  drop b\n  dup a\n  drop a\n  ret a", None),
        (
            "  drop a\n  drop b\n  let c = prim bytes_new(8)\n  ret c",
            None,
        ),
        ("  drop b\n  let c = alloc Box.B(a)\n  ret c", None),
        (
            "  drop b\n  let c = prim array_unique(a)\n  let w = prim array_set(c, i, c)\n  ret c",
            None,
        ),
    ] {
        assert_eq!(kept_by(body), [want], "{body}");
    }
    // Only owned pointers.
    let borrowed = "fn f(a: ptr @borrowed) -> ptr {\nentry:\n  ret a\n}\n";
    assert_eq!(kept(borrowed, "f"), [None]);
    let number = "fn f(a: ptr @owned) -> i64 {\nentry:\n  drop a\n  ret 1\n}\n";
    assert_eq!(kept(number, "f"), [None]);
}

#[test]
fn kept_results_hold_on_every_path() {
    let paths = |x: &str, y: &str| {
        kept_by(&format!(
            "  let d = prim eq(i, 0)\n  switch d {{ 0 => x, default => y }}\nx:\n{x}\ny:\n{y}"
        ))
    };
    let a = "  drop b\n  ret a";
    assert_eq!(paths(a, "  drop a\n  ret b"), [None]);
    assert_eq!(paths(a, "  drop b\n  panic \"no\""), [Some(0)]);
    assert_eq!(
        paths(a, "  drop b\n  let c = prim bytes_unique(a)\n  ret c"),
        [Some(0)]
    );
    // Passed through a block parameter it is lost.
    assert_eq!(
        kept_by("  drop b\n  jump x(a)\nx(p: ptr):\n  ret p"),
        [None]
    );
}

#[test]
fn kept_results_of_several_results_name_each_parameter() {
    let f = "fn f(a: ptr @owned, b: ptr @owned, i: i64) -> (i64, ptr, ptr) {\nentry:\n  \
             ret (i, b, a)\n}\n";
    assert_eq!(kept(f, "f"), [None, Some(1), Some(0)]);
}

/// A recursive `f` that returns `a` on the way out.
fn recursive(step: &str) -> String {
    format!(
        "fn f(a: ptr @owned, b: ptr @owned, i: i64) -> ptr {{\nentry:\n  \
         let d = prim eq(i, 0)\n  switch d {{ 0 => more, default => done }}\n\
         done:\n  drop b\n  ret a\nmore:\n  let j = prim sub(i, 1)\n{step}\n}}\n"
    )
}

#[test]
fn kept_results_follow_recursion() {
    // A loop, and a call that is not in tail position as in quicksort.
    assert_eq!(kept(&recursive("  tailcall f(a, b, j)"), "f"), [Some(0)]);
    let dupped = "  dup b\n  let l = call f(a, b, j)\n  let c = prim bytes_unique(l)\n  \
                  let w = prim bytes_set_i64(c, 0, 1)\n  tailcall f(c, b, j)";
    assert_eq!(kept(&recursive(dupped), "f"), [Some(0)]);
    // The next iteration gets the other array, or a call returns it.
    assert_eq!(kept(&recursive("  tailcall f(b, a, j)"), "f"), [None]);
    assert_eq!(
        kept(&recursive("  let l = call f(b, a, j)\n  ret l"), "f"),
        [None]
    );
}

#[test]
fn kept_results_follow_other_functions() {
    let src = r#"
fn f(a: ptr @owned) -> ptr {
entry:
  let r = call g(a)
  ret r
}

fn g(a: ptr @owned) -> ptr {
entry:
  tailcall id(a)
}

fn id(a: ptr @owned) -> ptr {
entry:
  ret a
}

fn pair(a: ptr @owned) -> (i64, ptr) {
entry:
  ret (1, a)
}

fn second(a: ptr @owned) -> ptr {
entry:
  let (x, r) = call pair(a)
  ret r
}

fn lost(a: ptr @owned) -> ptr {
entry:
  drop a
  let n = prim bytes_new(8)
  ret n
}

fn h(a: ptr @owned) -> ptr {
entry:
  let r = call lost(a)
  ret r
}

fn k(a: ptr @owned) -> ptr {
entry:
  tailcall lost(a)
}

fn p(a: ptr @owned, i: i64) -> ptr {
entry:
  let d = prim eq(i, 0)
  switch d { 0 => more, default => done }
done:
  ret a
more:
  let j = prim sub(i, 1)
  tailcall q(a, j)
}

fn q(a: ptr @owned, i: i64) -> ptr {
entry:
  tailcall p(a, i)
}
"#;
    assert_eq!(kept(src, "f"), [Some(0)]);
    assert_eq!(kept(src, "g"), [Some(0)]);
    assert_eq!(kept(src, "second"), [Some(0)]);
    assert_eq!(kept(src, "h"), [None]);
    assert_eq!(kept(src, "k"), [None]);
    // Cycles of different functions are not proved.
    assert_eq!(kept(src, "p"), [None]);
    assert_eq!(kept(src, "q"), [None]);
}

#[test]
fn kept_results_name_the_result_and_the_argument_of_the_call() {
    let src = r#"
fn swap(a: ptr @owned, b: ptr @owned) -> (ptr, ptr) {
entry:
  ret (b, a)
}

fn f(a: ptr @owned, b: ptr @owned) -> (ptr, ptr) {
entry:
  tailcall swap(a, b)
}

fn g(a: ptr @owned, b: ptr @owned) -> ptr {
entry:
  let (x, y) = call swap(a, b)
  drop y
  ret x
}

fn pick(y: ptr @owned, x: ptr @borrowed) -> ptr {
entry:
  ret y
}

fn twice(a: ptr @owned) -> ptr {
entry:
  let r = call pick(a, a)
  ret r
}
"#;
    assert_eq!(kept(src, "f"), [Some(1), Some(0)]);
    // `a` is dropped as `y` here: the result is `b`.
    assert_eq!(kept(src, "g"), [Some(1)]);
    // The callee may also see the array through the other argument.
    assert_eq!(kept(src, "twice"), [None]);
}

/// The functions that call `g` directly (not in tail position).
fn direct_callers(m: &Module, g: &str) -> Vec<String> {
    let calls = |ins: &Instr| match ins {
        Instr::Let {
            rhs: Rhs::Call { func, .. },
            ..
        }
        | Instr::LetMulti { func, .. } => func == g,
        _ => false,
    };
    m.funcs
        .iter()
        .filter(|f| f.blocks.iter().any(|b| b.instrs.iter().any(calls)))
        .map(|f| f.name.clone())
        .collect()
}

/// A loop over a loop, as in fannkuch-redux: `flips` passes its array
/// through `rev`, a specialized function declared later.
const THROUGH_LOOP: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(32)
  let w = prim bytes_set_i64(a, 0, 2)
  let v = prim bytes_set_i64(a, 1, 1)
  let u = prim bytes_set_i64(a, 2, 3)
  ret a
}

fn flips(p: ptr @owned, f: i64) -> (ptr, i64) {
entry:
  let ev = force p
  let k = prim bytes_get_i64(ev, 0)
  switch k { 0 => done, default => more }
done:
  ret (ev, f)
more:
  let r = call rev(ev, 0, k)
  let g = prim add(f, 1)
  tailcall flips(r, g)
}

fn rev(a: ptr @owned, lo: i64, hi: i64) -> ptr {
entry:
  let d = prim ge(lo, hi)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let c = prim bytes_unique(a)
  let x = prim bytes_get_i64(c, lo)
  let y = prim bytes_get_i64(c, hi)
  let w = prim bytes_set_i64(c, lo, y)
  let v = prim bytes_set_i64(c, hi, x)
  let l = prim add(lo, 1)
  let h = prim sub(hi, 1)
  tailcall rev(c, l, h)
}

fn main() -> i64 {
entry:
  let t = ARRAY
  BEFORE
  let (r, n) = call flips(t, 0)
  AFTER
  let x = prim bytes_get_i64(r, 1)
  drop r
  let _ = extern cirka_write_i64(n)
  let _ = extern cirka_write_i64(x)
  ret 0
}
"#;

fn through_loop(array: &str, before: &str, after: &str) -> String {
    THROUGH_LOOP
        .replace("ARRAY", array)
        .replace("  BEFORE\n", before)
        .replace("  AFTER\n", after)
}

#[test]
fn loops_are_specialized_through_loops_they_call() {
    let m = entered(&through_loop("call mk()", "", ""), 2);
    let u = func_text(&m, "flips$u");
    assert!(u.contains("call rev$u(p, 0, k)"), "{u}");
    assert!(u.contains("tailcall flips$u(r, g)"), "{u}");
    assert!(!u.contains("force"), "{u}");
    // The test at the entry of the outer loop replaces those of the inner.
    let f = func_text(&m, "flips");
    assert!(f.contains("prim is_value(p)"), "{f}");
    assert!(f.contains("prim is_unique(p)"), "{f}");
    // The cold code stays away from the hot functions.
    assert_eq!(direct_callers(&m, "rev$u"), ["flips$u"]);
    assert_eq!(direct_callers(&m, "rev$slow"), ["flips$slow"]);
    assert_eq!(direct_callers(&m, "rev$slow$u"), ["flips$slow$u"]);
    assert_eq!(hot_callers(&m, "flips"), Vec::<String>::new());
    assert_eq!(hot_callers(&m, "rev"), Vec::<String>::new());
    // Nothing checks the array in `flips`: after an iteration in the old
    // body the test of the entry is repeated, into the copies of the cold
    // path.
    let slow = func_text(&m, "flips$slow");
    assert!(slow.contains("tailcall flips$again(r, g)"), "{slow}");
    assert!(m.func("flips$again").unwrap().cold);
    let again = func_text(&m, "flips$again");
    assert!(again.contains("prim is_value(p)"), "{again}");
    assert!(again.contains("prim is_unique(p)"), "{again}");
    assert!(again.contains("tailcall flips$slow$u(p, f)"), "{again}");
    assert!(again.contains("tailcall flips$slow(p, f)"), "{again}");
    let evaluated = "  dup t\n  let v = force t\n  drop v\n";
    let read = "  let y = force t\n  let z = prim bytes_get_i64(y, 0)\n  drop y\n  \
                let _ = extern cirka_write_i64(z)\n";
    for (array, before, after, out) in [
        ("call mk()", "", "", "22"),
        ("mkthunk mk()", "", "", "22"),
        ("mkthunk mk()", evaluated, "", "22"),
        // The other owner must not see the writes.
        ("call mk()", "  dup t\n", read, "222"),
        ("mkthunk mk()", "  dup t\n", read, "222"),
    ] {
        let m = entered(&through_loop(array, before, after), 2);
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!(r.stdout, out, "{array} {before}");
        assert_eq!(r.leaked, 0, "{array} {before}");
    }
}

#[test]
fn the_repeated_test_goes_to_the_copy_or_the_old_body() {
    // Before the entries are split, it joins the specialized loop.
    let src = through_loop("mkthunk mk()", "", "");
    let mut m = parse(FileId(0), &src).unwrap();
    assert_eq!(crate::uniq::specialize_loops(&mut m).len(), 2);
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let again = func_text(&m, "flips$again");
    assert!(again.contains("tailcall flips$u(p, f)"), "{again}");
    assert!(again.contains("tailcall flips(p, f)"), "{again}");
    assert!(func_text(&m, "flips").contains("tailcall flips$again(r, g)"));
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!((r.stdout.as_str(), r.leaked), ("22", 0));
}

/// A loop whose iterations pass the array through other functions. `BODY`
/// is the iteration, and `NEXT` the array it passes on.
const THROUGH_CALL: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(16)
  let w = prim bytes_set_i64(a, 1, 5)
  ret a
}

fn id(a: ptr @owned) -> ptr {
entry:
  ret a
}

fn ev(a: ptr @owned) -> ptr {
entry:
  let e = force a
  ret e
}

fn pair(a: ptr @owned) -> (i64, ptr) {
entry:
  ret (9, a)
}

fn other(a: ptr @owned) -> ptr {
entry:
  drop a
  let n = prim bytes_new(16)
  ret n
}

fn hold(s: ptr @owned) -> ptr {
entry:
  ret s
}

fn fill(a: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  BODY
  let j = prim add(i, 1)
  tailcall fill(NEXT, j, n)
}

fn main() -> i64 {
entry:
  ENTRY
  let r = call fill(t, 0, 2)
  AFTER
  let x = prim bytes_get_i64(r, 1)
  drop r
  let _ = extern cirka_write_i64(x)
  ret 0
}
"#;

fn through_call(body: &str, next: &str, entry: &str, after: &str) -> String {
    THROUGH_CALL
        .replace("  BODY\n", body)
        .replace("NEXT", next)
        .replace("  ENTRY\n", entry)
        .replace("  AFTER\n", after)
}

const FRESH: &str = "  let t = call mk()\n";
const LAZY: &str = "  let t = mkthunk mk()\n";

#[test]
fn checked_arrays_stay_checked_across_calls_that_keep_them() {
    let body = "  let e = force a\n  let c = prim bytes_unique(e)\n  \
                let w = prim bytes_set_i64(c, i, 9)\n  let r = call id(c)\n";
    for entry in [FRESH, LAZY] {
        let m = entered(&through_call(body, "r", entry, ""), 1);
        let slow = func_text(&m, "fill$slow");
        assert!(slow.contains("tailcall fill$slow$u(r, j, n)"), "{slow}");
        let u = func_text(&m, "fill$u");
        assert!(!u.contains("bytes_unique"), "{u}");
        assert!(u.contains("tailcall fill$u(r, j, n)"), "{u}");
        let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
        assert_eq!((r.stdout.as_str(), r.leaked), ("9", 0));
    }
}

#[test]
fn arrays_returned_by_calls_need_no_checks() {
    // Also for several results and for chains of removed values.
    for (body, write) in [
        (
            "  let e = force a\n  let r = call id(e)\n  let c = prim bytes_unique(r)\n  \
             let w = prim bytes_set_i64(c, i, 9)\n",
            "bytes_set_i64(r, i, 9)",
        ),
        (
            "  let e = force a\n  let (q, r) = call pair(e)\n  let c = prim bytes_unique(r)\n  \
             let w = prim bytes_set_i64(c, i, q)\n",
            "bytes_set_i64(r, i, q)",
        ),
        (
            "  let r = call id(a)\n  let e = force r\n  let c = prim bytes_unique(e)\n  \
             let w = prim bytes_set_i64(c, i, 9)\n",
            "bytes_set_i64(r, i, 9)",
        ),
    ] {
        for entry in [FRESH, LAZY] {
            let m = entered(&through_call(body, "c", entry, ""), 1);
            let u = func_text(&m, "fill$u");
            assert!(!u.contains("bytes_unique"), "{u}");
            assert!(!u.contains("force"), "{u}");
            assert!(u.contains(write), "{u}");
            assert!(u.contains("tailcall fill$u(r, j, n)"), "{u}");
            let f = func_text(&m, "fill");
            assert!(f.contains("prim is_value(a)"), "{f}");
            assert!(f.contains("prim is_unique(a)"), "{f}");
            let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
            assert_eq!((r.stdout.as_str(), r.leaked), ("9", 0), "{body}");
        }
    }
}

#[test]
fn a_kept_result_says_nothing_about_a_thunk() {
    // The thunk is unique, what it evaluates to is not: `ev` returns the
    // array the other owner holds. The entry must not take the thunk for
    // the array.
    let body = "  let r = call ev(a)\n  let c = prim bytes_unique(r)\n  \
                let w = prim bytes_set_i64(c, i, 9)\n";
    let entry = "  let s = prim bytes_new(16)\n  dup s\n  let t = mkthunk hold(s)\n";
    let after = "  let y = prim bytes_get_i64(s, 0)\n  drop s\n  \
                 let _ = extern cirka_write_i64(y)\n";
    let m = entered(&through_call(body, "c", entry, after), 1);
    let f = func_text(&m, "fill");
    assert!(f.contains("prim is_value(a)"), "{f}");
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!((r.stdout.as_str(), r.leaked), ("09", 0));
}

#[test]
fn arrays_are_lost_in_calls_that_do_not_keep_them() {
    let body = "  let r = call other(a)\n  let c = prim bytes_unique(r)\n  \
                let w = prim bytes_set_i64(c, i, 9)\n";
    let m = entered(&through_call(body, "c", FRESH, ""), 0);
    assert!(m.func("fill$u").is_none());
}

/// `g` needs `a` to be a value and unique, and `use_it` shows why.
const CONSUMING_LOOP: &str = r#"
extern fn cirka_write_i64(i64) -> void

fn mk() -> ptr {
entry:
  let a = prim bytes_new(16)
  let w = prim bytes_set_i64(a, 1, 5)
  ret a
}

fn g(a: ptr @owned, i: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, 2)
  switch d { 0 => body, default => done }
done:
  drop a
  ret acc
body:
  let ev = force a
  let c = prim bytes_unique(ev)
  let x = prim bytes_get_i64(c, i)
  let w = prim bytes_set_i64(c, i, 1)
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall g(c, j, acc1)
}

fn use_it(a: ptr @owned, i: i64, n: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => more, default => done }
done:
  let x = call g(a, 0, 0)
  ret x
more:
  let j = prim add(i, 1)
  tailcall use_it(a, j, n)
}

fn main() -> i64 {
entry:
  let t = ARRAY
  let r = call use_it(t, 0, 2)
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;

#[test]
fn callers_test_what_the_copies_they_call_assume() {
    let m = entered(&CONSUMING_LOOP.replace("ARRAY", "mkthunk mk()"), 2);
    let f = func_text(&m, "use_it");
    assert!(f.contains("prim is_value(a)"), "{f}");
    assert!(f.contains("prim is_unique(a)"), "{f}");
    assert!(func_text(&m, "use_it$u").contains("call g$u(a, 0, 0)"));
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "5");
    let m = entered(&CONSUMING_LOOP.replace("ARRAY", "call mk()"), 2);
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "5");
}

#[test]
fn copies_are_called_only_when_their_assumptions_hold() {
    // `f` assumes only that its array is a value, `g$u` a unique one.
    let src = r#"
extern fn cirka_write_i64(i64) -> void

fn g(a: ptr @owned, i: i64) -> ptr {
entry:
  let d = prim ge(i, 2)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let c = prim bytes_unique(a)
  let w = prim bytes_set_i64(c, i, 1)
  let j = prim add(i, 1)
  tailcall g(c, j)
}

fn f(a: ptr @owned, i: i64, n: i64, acc: i64) -> i64 {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop a
  ret acc
body:
  let ev = force a
  dup ev
  let r = call g(ev, 0)
  let x = prim bytes_get_i64(r, 0)
  drop r
  let acc1 = prim add(acc, x)
  let j = prim add(i, 1)
  tailcall f(ev, j, n, acc1)
}

fn main() -> i64 {
entry:
  let t = prim bytes_new(16)
  let r = call f(t, 0, 2, 0)
  let _ = extern cirka_write_i64(r)
  ret 0
}
"#;
    let m = entered(src, 2);
    let u = func_text(&m, "f$u");
    assert!(
        u.contains("call g(ev, 0)") || u.contains("call g(a, 0)"),
        "{u}"
    );
    assert_eq!(direct_callers(&m, "g$u"), Vec::<String>::new());
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "2");
}

/// `g$u` assumes both of its arrays; `f` can vouch only for the first.
#[test]
fn assumptions_that_remove_nothing_are_dropped() {
    let src = r#"
extern fn cirka_write_i64(i64) -> void

fn g(a: ptr @owned, b: ptr @owned, i: i64) -> ptr {
entry:
  let d = prim ge(i, 2)
  switch d { 0 => body, default => done }
done:
  drop b
  ret a
body:
  let c = prim bytes_unique(a)
  let e = prim bytes_unique(b)
  let w = prim bytes_set_i64(c, i, 1)
  let v = prim bytes_set_i64(e, i, 2)
  let j = prim add(i, 1)
  tailcall g(c, e, j)
}

fn f(x: ptr @owned, y: ptr @owned, i: i64, n: i64) -> ptr {
entry:
  let d = prim ge(i, n)
  switch d { 0 => body, default => done }
done:
  drop y
  ret x
body:
  dup y
  let r = call g(x, y, 0)
  let j = prim add(i, 1)
  tailcall f(r, y, j, n)
}

fn main() -> i64 {
entry:
  let x = prim bytes_new(16)
  let y = prim bytes_new(16)
  let r = call f(x, y, 0, 2)
  let v = prim bytes_get_i64(r, 1)
  drop r
  let _ = extern cirka_write_i64(v)
  ret 0
}
"#;
    let m = entered(src, 1);
    assert!(m.func("f$u").is_none());
}

#[test]
fn recursive_calls_of_the_copy_go_to_the_copy() {
    // As in quicksort: one recursive call in tail position, one not.
    let src = r#"
extern fn cirka_write_i64(i64) -> void

fn sort(a: ptr @owned, i: i64) -> ptr {
entry:
  let d = prim eq(i, 0)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let c = prim bytes_unique(a)
  let w = prim bytes_set_i64(c, 0, i)
  let j = prim sub(i, 1)
  let l = call sort(c, j)
  tailcall sort(l, j)
}

fn main() -> i64 {
entry:
  let t = prim bytes_new(16)
  let r = call sort(t, 3)
  let x = prim bytes_get_i64(r, 0)
  drop r
  let _ = extern cirka_write_i64(x)
  ret 0
}
"#;
    let m = entered(src, 1);
    let u = func_text(&m, "sort$u");
    assert!(u.contains("call sort$u(a, j)"), "{u}");
    assert!(u.contains("tailcall sort$u(l, j)"), "{u}");
    assert!(!u.contains("bytes_unique"), "{u}");
    assert_eq!(direct_callers(&m, "sort$u"), ["sort$u"]);
    // The old body stays on the cold path.
    let slow = func_text(&m, "sort$slow");
    assert!(slow.contains("call sort$slow(c, j)"), "{slow}");
    assert_eq!(direct_callers(&m, "sort$slow$u"), ["sort$slow$u"]);
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().stdout, "1");
}

#[test]
fn recursive_calls_with_other_arrays_stay_at_the_entry() {
    // The recursive call gets a shared array: it needs the check.
    let src = r#"
extern fn cirka_write_i64(i64) -> void

fn sort(a: ptr @owned, i: i64) -> ptr {
entry:
  let d = prim eq(i, 0)
  switch d { 0 => body, default => done }
done:
  ret a
body:
  let c = prim bytes_unique(a)
  let w = prim bytes_set_i64(c, 0, i)
  let j = prim sub(i, 1)
  let n = prim bytes_new(16)
  dup n
  let l = call sort(n, j)
  drop l
  let x = prim bytes_get_i64(n, 0)
  drop n
  let _ = extern cirka_write_i64(x)
  tailcall sort(c, j)
}

fn main() -> i64 {
entry:
  let t = prim bytes_new(16)
  let r = call sort(t, 3)
  drop r
  ret 0
}
"#;
    let m = entered(src, 1);
    let u = func_text(&m, "sort$u");
    assert!(u.contains("call sort(n, j)"), "{u}");
    assert_eq!(crate::interp::run(&m, b"", 1_000_000).unwrap().leaked, 0);
}

#[test]
fn cold_functions_round_trip_through_the_text_format() {
    let src = "cold fn f(x: i64) -> i64 {\nentry:\n  ret x\n}\n\nfn main() -> i64 {\nentry:\n  let r = call f(1)\n  ret r\n}\n";
    let m = parse(FileId(0), src).unwrap();
    assert!(m.func("f").unwrap().cold);
    assert!(!m.func("main").unwrap().cold);
    let printed = print_module(&m);
    assert!(printed.contains("cold fn f(x: i64)"), "{printed}");
    assert!(parse(FileId(0), &printed).unwrap().func("f").unwrap().cold);
}

// ---- BUG-0014: lazily bound constructors ----

/// Lowers a program that binds `(con B () field)` lazily and uses it in one
/// branch only, and tells whether it builds a thunk for the constructor.
fn lazy_con_makes_thunk(field: &str) -> bool {
    let src = format!(
        "(data Box () (B (! Int#)))
         (def pick (-> Int# Int#)
           (lam ((n Int#))
             (let ((a Box (con B () {field})))
               (case n (k Int#) Int#
                 (0 3)
                 (_ (case a (s Box) Int# ((B v) v)))))))
         (def main Int# (pick 0))"
    );
    let core = cirka_core::parse_program(FileId(0), &src).unwrap();
    let m = lower::lower_program(&core).unwrap();
    print_module(&m).contains("mkthunk")
}

#[test]
fn lazy_constructors_wait_only_for_unsafe_strict_fields() {
    // Adding cannot fail, so the box is built where it is bound.
    assert!(!lazy_con_makes_thunk("(prim add n 1)"));
    // Dividing can: it has to wait until something demands the box.
    assert!(lazy_con_makes_thunk("(prim quot 7 n)"));
}

/// A drop before a `switch` passes its reuse token to the arms (through
/// nested switches): an arm that allocates a cell of the same size reuses
/// it, the others free it (FEAT-0079).
#[test]
fn reuse_crosses_switch_arms() {
    let src = r#"
type P { P = 0 { a: i64, b: i64 } }
fn step(p: ptr @owned, k: i64) -> i64 {
entry:
  switch p { P.P => body }
body:
  let a = proj p P.P.a
  let b = proj p P.P.b
  drop p
  switch k { 0 => done, default => more }
done:
  ret a
more:
  let c = prim lt(a, b)
  switch c { 0 => again, default => last }
again:
  let q = alloc P.P(b, a)
  let k2 = prim sub(k, 1)
  let r = call step(q, k2)
  ret r
last:
  ret b
}
fn main() -> i64 {
entry:
  let p = alloc P.P(5, 2)
  let r = call step(p, 3)
  ret r
}
"#;
    let mut m = parse(FileId(0), src).unwrap();
    crate::reuse::optimize(&mut m);
    let errs = crate::verify::verify_module(&m);
    assert!(errs.is_empty(), "{}", crate::verify::render_errors(&errs));
    let out = print_module(&m);
    assert!(out.contains("drop_reuse p"), "{out}");
    assert!(out.contains("alloc P.P(b, a) reuse"), "{out}");
    let f = m.func("step").unwrap();
    let freed = f
        .blocks
        .iter()
        .filter(|b| matches!(b.instrs.first(), Some(Instr::Drop(v)) if f.var_name(*v).starts_with("ru")))
        .count();
    assert_eq!(freed, 2, "{out}");
    let r = crate::interp::run(&m, b"", 1_000_000).unwrap();
    assert_eq!(r.leaked, 0);
    assert_eq!(r.stats.allocs, 1, "{:?}", r.stats);
}
