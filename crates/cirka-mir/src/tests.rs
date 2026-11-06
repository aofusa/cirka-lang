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
    assert_eq!(crate::uniq::specialize_loops(&mut m), 1);
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
    assert_eq!(crate::uniq::specialize_loops(&mut m), 0);
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
        assert_eq!(crate::uniq::specialize_loops(&mut m), 0, "{zero} first");
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
    assert_eq!(crate::uniq::specialize_loops(&mut m), 0);
    assert!(crate::interp::run(&m, b"", 1_000_000).is_ok());
    // In-place reads keep it unique.
    let src = UNIQUE_LOOP.replace(
        "  let c = prim bytes_unique(a)\n",
        "  let c = prim bytes_unique(a)\n  let k = prim bytes_len(c)\n",
    );
    let mut m = parse(FileId(0), &src).unwrap();
    assert_eq!(crate::uniq::specialize_loops(&mut m), 1);
    // Only unchecked parameters reach the tail call: nothing is known.
    let src = UNIQUE_LOOP
        .replace("  let c = prim bytes_unique(a)\n", "")
        .replace("bytes_set_i64(c, i, y)", "bytes_get_i64(a, i)")
        .replace("tailcall fill(c, j, n)", "tailcall fill(a, j, n)");
    let mut m = parse(FileId(0), &src).unwrap();
    assert_eq!(crate::uniq::specialize_loops(&mut m), 0);
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
    assert_eq!(crate::uniq::specialize_loops(&mut m), 1);
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
