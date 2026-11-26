//! Foreign calls (08-ffi.md §7.2): C signatures lowered to LLVM as clang
//! lowers them for x86_64 System V. Aggregates in registers are moved as
//! one LLVM value per eightbyte (`i64`, `i24`, `double`, `<2 x float>`),
//! aggregates in memory as `byval` arguments and `sret` results. The Core
//! value of an aggregate is the address of its bytes.

use cirka_mir::{CSig, CTy, CType, CTypes, Pass};
use std::fmt::Write;

/// A C signature lowered to LLVM.
pub(crate) struct Lowered {
    /// How each C parameter is passed.
    pub params: Vec<Pass>,
    pub ret: Pass,
}

pub(crate) fn lower(ct: CTypes<'_>, sig: &CSig) -> Lowered {
    let (params, ret) = ct
        .assign(&sig.params, &sig.ret)
        .expect("ICE: C signature with an unknown type (MIR verification)");
    Lowered { params, ret }
}

/// Size and alignment of an aggregate (`[N x i8]` with `align A`).
pub(crate) fn size_align(ct: CTypes<'_>, t: &CType) -> (u64, u64) {
    ct.size_align(t)
        .expect("ICE: C aggregate without a layout (MIR verification)")
}

/// The LLVM type of a C scalar.
pub(crate) fn c_ll(t: CTy) -> &'static str {
    use CTy::*;
    match t {
        I8 | U8 => "i8",
        I16 | U16 => "i16",
        I32 | U32 => "i32",
        I64 | U64 => "i64",
        F32 => "float",
        F64 => "double",
        Ptr => "ptr",
        Void => "void",
    }
}

/// The extension attribute System V (as clang and gcc implement it)
/// expects for a small integer, with a trailing space.
fn ext(t: CTy) -> &'static str {
    use CTy::*;
    match t {
        I8 | I16 => "signext ",
        U8 | U16 => "zeroext ",
        _ => "",
    }
}

/// A C parameter type with its extension attribute.
pub(crate) fn c_param(t: CTy) -> String {
    match ext(t) {
        "" => c_ll(t).to_string(),
        a => format!("{} {}", c_ll(t), a.trim_end()),
    }
}

/// The LLVM result type, and its attributes (with a trailing space).
pub(crate) fn ret_ty(low: &Lowered) -> (String, &'static str) {
    match &low.ret {
        Pass::Scalar(t) => (c_ll(*t).to_string(), ext(*t)),
        Pass::Memory => ("void".into(), ""),
        Pass::Regs(ps) => match ps.as_slice() {
            [p] => (p.llvm(), ""),
            _ => {
                let ts: Vec<String> = ps.iter().map(|p| p.llvm()).collect();
                (format!("{{ {} }}", ts.join(", ")), "")
            }
        },
    }
}

/// The declaration `declare ... @sym(...)` of a C function.
pub(crate) fn declaration(ct: CTypes<'_>, sym: &str, sig: &CSig) -> String {
    let low = lower(ct, sig);
    let (rt, rattr) = ret_ty(&low);
    let mut ps = Vec::new();
    if low.ret == Pass::Memory {
        let (n, a) = size_align(ct, &sig.ret);
        ps.push(format!("ptr sret([{n} x i8]) align {}", a.max(8)));
    }
    let fixed = sig.variadic.unwrap_or(sig.params.len());
    for (t, pass) in sig.params.iter().zip(&low.params).take(fixed) {
        match pass {
            Pass::Scalar(s) => ps.push(c_param(*s)),
            Pass::Regs(pieces) => ps.extend(pieces.iter().map(|p| p.llvm())),
            Pass::Memory => {
                let (n, a) = size_align(ct, t);
                ps.push(format!("ptr byval([{n} x i8]) align {}", a.max(8)));
            }
        }
    }
    if sig.variadic.is_some() {
        ps.push("...".into());
    }
    format!("declare {rattr}{rt} @{sym}({})", ps.join(", "))
}

/// Emits code into `lines` with fresh temporaries from `fresh`.
pub(crate) struct Emit<'a> {
    pub lines: &'a mut Vec<String>,
    pub fresh: &'a mut dyn FnMut() -> String,
}

impl Emit<'_> {
    fn t(&mut self) -> String {
        (self.fresh)()
    }

    fn line(&mut self, s: String) {
        self.lines.push(s);
    }

    /// `base + off` as a pointer.
    fn at(&mut self, base: &str, off: u64) -> String {
        if off == 0 {
            return base.to_string();
        }
        let p = self.t();
        self.line(format!(
            "{p} = getelementptr inbounds i8, ptr {base}, i64 {off}"
        ));
        p
    }

    fn inttoptr(&mut self, v: &str) -> String {
        let p = self.t();
        self.line(format!("{p} = inttoptr i64 {v} to ptr"));
        p
    }
}

/// A call of a C function. `callee` is `@sym` or a pointer value; `ops`
/// are the Core operands (`i64` / `double` text): the result's address
/// first for an aggregate result, then one per C parameter. A scalar
/// result is defined as `dst` (`i64` / `double`); returns whether it was.
pub(crate) fn call(
    ct: CTypes<'_>,
    e: &mut Emit<'_>,
    dst: &str,
    callee: &str,
    sig: &CSig,
    explicit_type: bool,
    ops: &[String],
) -> bool {
    let low = lower(ct, sig);
    let agg_ret = sig.ret.is_agg();
    let out = if agg_ret {
        Some(e.inttoptr(&ops[0]))
    } else {
        None
    };
    let args = &ops[usize::from(agg_ret)..];
    let mut parts = Vec::new();
    let mut tys: Vec<String> = Vec::new();
    if low.ret == Pass::Memory {
        let (n, a) = size_align(ct, &sig.ret);
        parts.push(format!(
            "ptr sret([{n} x i8]) align {} {}",
            a.max(8),
            out.as_ref().unwrap()
        ));
        tys.push("ptr".into());
    }
    let fixed = sig.variadic.unwrap_or(sig.params.len());
    for (i, ((t, pass), o)) in sig.params.iter().zip(&low.params).zip(args).enumerate() {
        let is_fixed = i < fixed;
        match pass {
            Pass::Scalar(s) => {
                let v = match s {
                    CTy::I64 | CTy::U64 | CTy::F64 => o.clone(),
                    CTy::Ptr => e.inttoptr(o),
                    CTy::F32 => {
                        let x = e.t();
                        e.line(format!("{x} = fptrunc double {o} to float"));
                        x
                    }
                    s => {
                        let x = e.t();
                        e.line(format!("{x} = trunc i64 {o} to {}", c_ll(*s)));
                        x
                    }
                };
                parts.push(format!("{} {v}", c_param(*s)));
                if is_fixed {
                    tys.push(c_ll(*s).into());
                }
            }
            Pass::Regs(pieces) => {
                let (_, a) = size_align(ct, t);
                let p = e.inttoptr(o);
                for (k, piece) in pieces.iter().enumerate() {
                    let at = e.at(&p, 8 * k as u64);
                    let v = e.t();
                    let ty = piece.llvm();
                    e.line(format!("{v} = load {ty}, ptr {at}, align {a}"));
                    parts.push(format!("{ty} {v}"));
                    if is_fixed {
                        tys.push(ty);
                    }
                }
            }
            Pass::Memory => {
                let (n, a) = size_align(ct, t);
                let p = e.inttoptr(o);
                parts.push(format!("ptr byval([{n} x i8]) align {} {p}", a.max(8)));
                if is_fixed {
                    tys.push("ptr".into());
                }
            }
        }
    }
    let (rt, rattr) = ret_ty(&low);
    let fnty = if sig.variadic.is_some() {
        if tys.is_empty() {
            format!("{rt} (...)")
        } else {
            format!("{rt} ({}, ...)", tys.join(", "))
        }
    } else if explicit_type {
        format!("{rt} ({})", tys.join(", "))
    } else {
        rt.clone()
    };
    let call = format!("call {rattr}{fnty} {callee}({})", parts.join(", "));
    match &low.ret {
        Pass::Scalar(CTy::Void) | Pass::Memory => {
            e.line(call);
            false
        }
        Pass::Scalar(CTy::I64 | CTy::U64 | CTy::F64) => {
            e.line(format!("{dst} = {call}"));
            true
        }
        Pass::Scalar(s) => {
            let x = e.t();
            e.line(format!("{x} = {call}"));
            let conv = match s {
                CTy::Ptr => format!("ptrtoint ptr {x} to i64"),
                CTy::F32 => format!("fpext float {x} to double"),
                s if s.is_signed() => format!("sext {} {x} to i64", c_ll(*s)),
                s => format!("zext {} {x} to i64", c_ll(*s)),
            };
            e.line(format!("{dst} = {conv}"));
            true
        }
        Pass::Regs(pieces) => {
            let (_, a) = size_align(ct, &sig.ret);
            let out = out.unwrap();
            let r = e.t();
            e.line(format!("{r} = {call}"));
            if let [p] = pieces.as_slice() {
                e.line(format!("store {} {r}, ptr {out}, align {a}", p.llvm()));
            } else {
                for (k, p) in pieces.iter().enumerate() {
                    let v = e.t();
                    e.line(format!("{v} = extractvalue {rt} {r}, {k}"));
                    let at = e.at(&out, 8 * k as u64);
                    e.line(format!("store {} {v}, ptr {at}, align {a}", p.llvm()));
                }
            }
            false
        }
    }
}

/// The trampoline `@"cb.<name>"` of a callback: a C function receiving
/// the environment in `r10` (`nest`) from the runtime's stub, calling the
/// handler `handler` (a `tailcc` MIR function taking the environment,
/// the result's address for an aggregate result, and the Core values of
/// the C arguments). `dup_env`: the handler owns its environment, and the
/// stub keeps its own reference. `catches`: the handler may raise, and an
/// exception it raises cannot unwind into C: the trampoline catches it and
/// ends the process (`cirka_uncaught_exit`, 05-runtime.md §6.3).
pub(crate) fn trampoline(
    ct: CTypes<'_>,
    name: &str,
    sig: &CSig,
    handler: &str,
    handler_ret: &str,
    dup_env: bool,
    catches: bool,
) -> String {
    let low = lower(ct, sig);
    let (rt, rattr) = ret_ty(&low);
    let mut params = vec!["ptr nest %env".to_string()];
    let mut body = Vec::new();
    let mut n = 0usize;
    let mut fresh = || {
        n += 1;
        format!("%t{n}")
    };
    let mut args = vec!["ptr %env".to_string()];
    let mut e = Emit {
        lines: &mut body,
        fresh: &mut fresh,
    };
    let out = match &low.ret {
        Pass::Memory => {
            let (n, a) = size_align(ct, &sig.ret);
            params.push(format!("ptr sret([{n} x i8]) align {} %sret", a.max(8)));
            Some("%sret".to_string())
        }
        Pass::Regs(_) => {
            let (n, a) = size_align(ct, &sig.ret);
            e.line(format!("%out = alloca [{n} x i8], align {}", a.max(8)));
            Some("%out".to_string())
        }
        Pass::Scalar(_) => None,
    };
    if let Some(o) = &out {
        let x = e.t();
        e.line(format!("{x} = ptrtoint ptr {o} to i64"));
        args.push(format!("i64 {x}"));
    }
    for (i, (t, pass)) in sig.params.iter().zip(&low.params).enumerate() {
        match pass {
            Pass::Scalar(s) => {
                let c = format!("%c{i}");
                params.push(format!("{} {c}", c_param(*s)));
                let v = match s {
                    CTy::I64 | CTy::U64 => format!("i64 {c}"),
                    CTy::F64 => format!("double {c}"),
                    CTy::Ptr => {
                        let x = e.t();
                        e.line(format!("{x} = ptrtoint ptr {c} to i64"));
                        format!("i64 {x}")
                    }
                    CTy::F32 => {
                        let x = e.t();
                        e.line(format!("{x} = fpext float {c} to double"));
                        format!("double {x}")
                    }
                    s => {
                        let x = e.t();
                        let op = if s.is_signed() { "sext" } else { "zext" };
                        e.line(format!("{x} = {op} {} {c} to i64", c_ll(*s)));
                        format!("i64 {x}")
                    }
                };
                args.push(v);
            }
            Pass::Regs(pieces) => {
                let (n, a) = size_align(ct, t);
                let m = format!("%m{i}");
                e.line(format!("{m} = alloca [{n} x i8], align {}", a.max(8)));
                for (k, p) in pieces.iter().enumerate() {
                    let c = format!("%c{i}.{k}");
                    params.push(format!("{} {c}", p.llvm()));
                    let at = e.at(&m, 8 * k as u64);
                    e.line(format!("store {} {c}, ptr {at}, align {a}", p.llvm()));
                }
                let x = e.t();
                e.line(format!("{x} = ptrtoint ptr {m} to i64"));
                args.push(format!("i64 {x}"));
            }
            Pass::Memory => {
                let (n, a) = size_align(ct, t);
                let c = format!("%c{i}");
                params.push(format!("ptr byval([{n} x i8]) align {} {c}", a.max(8)));
                let x = e.t();
                e.line(format!("{x} = ptrtoint ptr {c} to i64"));
                args.push(format!("i64 {x}"));
            }
        }
    }
    if dup_env {
        e.line("call void @cirka_dup(ptr %env)".into());
    }
    let r = e.t();
    if catches {
        e.line(format!(
            "{r} = invoke tailcc {handler_ret} {handler}({}) to label %ok unwind label %uncaught",
            args.join(", ")
        ));
        e.line("ok:".into());
    } else {
        e.line(format!(
            "{r} = call tailcc {handler_ret} {handler}({})",
            args.join(", ")
        ));
    }
    match &low.ret {
        Pass::Scalar(CTy::Void) | Pass::Memory => e.line("ret void".into()),
        Pass::Scalar(s) => {
            let v = match s {
                CTy::I64 | CTy::U64 | CTy::F64 => r.clone(),
                CTy::Ptr => {
                    let x = e.t();
                    e.line(format!("{x} = inttoptr i64 {r} to ptr"));
                    x
                }
                CTy::F32 => {
                    let x = e.t();
                    e.line(format!("{x} = fptrunc double {r} to float"));
                    x
                }
                s => {
                    let x = e.t();
                    e.line(format!("{x} = trunc i64 {r} to {}", c_ll(*s)));
                    x
                }
            };
            e.line(format!("ret {} {v}", c_ll(*s)));
        }
        Pass::Regs(pieces) => {
            let (_, a) = size_align(ct, &sig.ret);
            let out = out.unwrap();
            let mut acc = "undef".to_string();
            for (k, p) in pieces.iter().enumerate() {
                let at = e.at(&out, 8 * k as u64);
                let v = e.t();
                e.line(format!("{v} = load {}, ptr {at}, align {a}", p.llvm()));
                if pieces.len() == 1 {
                    acc = v;
                } else {
                    let x = e.t();
                    e.line(format!(
                        "{x} = insertvalue {rt} {acc}, {} {v}, {k}",
                        p.llvm()
                    ));
                    acc = x;
                }
            }
            e.line(format!("ret {rt} {acc}"));
        }
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "define internal {rattr}{rt} @\"cb.{name}\"({}) #0{} {{\nentry:",
        params.join(", "),
        if catches {
            " personality ptr @cirka_personality"
        } else {
            ""
        }
    );
    for l in body {
        let _ = writeln!(s, "  {l}");
    }
    if catches {
        s.push_str("uncaught:\n  %lp = landingpad { ptr, i32 } catch ptr null\n  %ue = extractvalue { ptr, i32 } %lp, 0\n  call void @cirka_uncaught_exit(ptr %ue)\n  unreachable\n");
    }
    s.push_str("}\n");
    s
}
