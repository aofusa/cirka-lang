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
