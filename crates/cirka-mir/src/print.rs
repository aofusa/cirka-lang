//! Pretty printer for the MIR text format. `parse(print(m)) == m` holds for
//! every module produced by the parser, and the MIR of the frontends reads
//! back too: names that are not identifiers are quoted, and variables or
//! blocks that share a name are told apart (BUG-0045).

use crate::ir::*;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// `s` as a name in the MIR text: as it is when it reads back as one
/// identifier, otherwise between backquotes with `` ` `` and `\` escaped
/// (operator names such as `Cirka.Base.++`).
pub fn name(s: &str) -> Cow<'_, str> {
    let b = s.as_bytes();
    if b.first().is_some_and(|&c| crate::parse::is_ident_start(c))
        && b.iter().all(|&c| crate::parse::is_ident_char(c))
    {
        return Cow::Borrowed(s);
    }
    let mut q = String::with_capacity(s.len() + 2);
    q.push('`');
    for c in s.chars() {
        if matches!(c, '`' | '\\') {
            q.push('\\');
        }
        q.push(c);
    }
    q.push('`');
    Cow::Owned(q)
}

/// One name per entry of `names`, all different: a name that an earlier
/// entry already has gets the first free `name%k` (the frontends give
/// several variables the same name; the parser's modules have distinct
/// names, which stay as they are). `_` is not a variable in the text.
fn distinct<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let names: Vec<&str> = names.collect();
    let mut taken: HashSet<String> = names.iter().map(|n| n.to_string()).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut next: HashMap<&str, usize> = HashMap::new();
    names
        .iter()
        .map(|&n| {
            if n != "_" && seen.insert(n) {
                return name(n).into_owned();
            }
            let k = next.entry(n).or_insert(1);
            loop {
                let c = format!("{n}%{k}");
                *k += 1;
                if taken.insert(c.clone()) {
                    return name(&c).into_owned();
                }
            }
        })
        .collect()
}

/// The names of the variables and blocks of a function in the text.
struct Names {
    vars: Vec<String>,
    labels: Vec<String>,
}

impl Names {
    fn of(f: &Function) -> Names {
        Names {
            vars: distinct(f.var_names.iter().map(String::as_str)),
            labels: distinct(f.blocks.iter().map(|b| b.label.as_str())),
        }
    }

    fn var(&self, v: Var) -> &str {
        &self.vars[v.0 as usize]
    }

    fn label(&self, b: BlockId) -> &str {
        &self.labels[b.0 as usize]
    }
}

/// A constructor path `Type.Ctor`, or a field path `Type.Ctor.field`.
fn path(m: &Module, c: CtorRef, field: Option<u32>) -> String {
    let mut p = m.ctor_name(c);
    if let Some(i) = field {
        p.push('.');
        p.push_str(&m.ctor(c).fields[i as usize].name);
    }
    name(&p).into_owned()
}

pub fn print_module(m: &Module) -> String {
    let mut s = String::new();
    for t in &m.types {
        let _ = write!(s, "type {} {{ ", name(&t.name));
        for (i, c) in t.ctors.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            let _ = write!(s, "{} = {} {{", name(&c.name), c.tag);
            for (j, f) in c.fields.iter().enumerate() {
                if j > 0 {
                    s.push(',');
                }
                let _ = write!(s, " {}: {}", name(&f.name), f.ty);
            }
            s.push_str(if c.fields.is_empty() { "}" } else { " }" });
        }
        s.push_str(" }\n");
    }
    for a in &m.caggs {
        let fs: Vec<String> = a.fields.iter().map(c_ty).collect();
        let _ = writeln!(
            s,
            "{} {}{} {{ {} }}",
            if a.union { "cunion" } else { "cstruct" },
            name(&a.name),
            if a.packed { " packed" } else { "" },
            fs.join(", ")
        );
    }
    for e in &m.externs {
        let _ = write!(s, "extern fn {}(", name(&e.name));
        for (i, (t, o)) in e.params.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            let _ = write!(s, "{t}");
            if *o == Own::Owned {
                s.push_str(" @owned");
            }
        }
        let _ = write!(s, ") -> {}", e.ret.map(|t| t.name()).unwrap_or("void"));
        if let Some(c) = &e.c_sig {
            let mut ps: Vec<String> = c.params.iter().map(c_ty).collect();
            if let Some(n) = c.variadic {
                ps.insert(n, "...".into());
            }
            let _ = write!(s, " as c({}) -> {}", ps.join(", "), c_ty(&c.ret));
        }
        if let Some(l) = &e.link {
            let _ = write!(s, " link {l}");
        }
        match &e.kind {
            cirka_core::ExternKind::Call => {}
            cirka_core::ExternKind::Dynamic => s.push_str(" dynamic"),
            cirka_core::ExternKind::Addr => s.push_str(" addr"),
            cirka_core::ExternKind::Callback(h) => {
                let _ = write!(s, " callback {}", name(h));
            }
        }
        s.push('\n');
    }
    for g in &m.globals {
        let _ = writeln!(s, "global {} = {}", name(&g.name), name(&g.init));
    }
    for f in &m.funcs {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(&print_function(m, f));
    }
    s
}

/// A C type in the MIR text form (`i32`, `timespec`, `array(f32, 4)`).
fn c_ty(t: &cirka_core::CType) -> String {
    match t {
        cirka_core::CType::Scalar(s) => s.name().to_string(),
        cirka_core::CType::Agg(n) => name(n).into_owned(),
        cirka_core::CType::Array(e, n) => format!("array({}, {n})", c_ty(e)),
    }
}

pub fn print_function(m: &Module, f: &Function) -> String {
    let n = Names::of(f);
    let mut s = String::new();
    let cold = if f.cold { "cold " } else { "" };
    let _ = write!(s, "{cold}fn {}(", name(&f.name));
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let _ = write!(s, "{}: {}", n.var(p.var), f.ty(p.var));
        if f.ty(p.var).is_ptr() {
            s.push_str(if p.own == Own::Owned {
                " @owned"
            } else {
                " @borrowed"
            });
        }
    }
    if f.is_multi() {
        let rs: Vec<&str> = f.rets.iter().map(|t| t.name()).collect();
        let _ = writeln!(s, ") -> ({}) {{", rs.join(", "));
    } else {
        let _ = writeln!(s, ") -> {} {{", f.ret);
    }
    for (bi, b) in f.blocks.iter().enumerate() {
        s.push_str(n.label(BlockId(bi as u32)));
        if !b.params.is_empty() {
            s.push('(');
            for (i, p) in b.params.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                let _ = write!(s, "{}: {}", n.var(*p), f.ty(*p));
            }
            s.push(')');
        }
        s.push_str(":\n");
        for ins in &b.instrs {
            s.push_str("  ");
            s.push_str(&print_instr(m, &n, ins));
            s.push('\n');
        }
        s.push_str("  ");
        s.push_str(&print_term(m, &n, &b.term));
        s.push('\n');
    }
    s.push_str("}\n");
    s
}

fn esc(bytes: &[u8]) -> String {
    let mut s = String::from("\"");
    for &b in bytes {
        match b {
            b'\n' => s.push_str("\\n"),
            b'\t' => s.push_str("\\t"),
            b'\r' => s.push_str("\\r"),
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            0x20..=0x7e => s.push(b as char),
            _ => {
                let _ = write!(s, "\\x{b:02x}");
            }
        }
    }
    s.push('"');
    s
}

pub fn print_const(c: &Const) -> String {
    match c {
        Const::Int(i, _) => i.to_string(),
        Const::Float(x) => {
            let mut t = format!("{x:?}");
            if !t.contains(['.', 'e', 'E']) {
                t.push_str(".0");
            }
            t
        }
        Const::Str(b) => esc(b),
    }
}

fn op(n: &Names, o: &Operand) -> String {
    match o {
        Operand::Var(v) => n.var(*v).to_string(),
        Operand::Const(c) => print_const(c),
    }
}

fn ops(n: &Names, os: &[Operand]) -> String {
    let v: Vec<String> = os.iter().map(|o| op(n, o)).collect();
    format!("({})", v.join(", "))
}

fn print_rhs(m: &Module, n: &Names, rhs: &Rhs) -> String {
    match rhs {
        Rhs::Lit(c) => format!("lit {}", print_const(c)),
        Rhs::Prim(p, args) => format!("prim {}{}", p.name(), ops(n, args)),
        Rhs::Alloc { ctor, args, reuse } => {
            let mut s = format!("alloc {}{}", path(m, *ctor, None), ops(n, args));
            if let Some(r) = reuse {
                let _ = write!(s, " reuse {}", n.var(*r));
            }
            s
        }
        Rhs::Proj { obj, ctor, field } => {
            format!("proj {} {}", n.var(*obj), path(m, *ctor, Some(*field)))
        }
        Rhs::Call { func, args } => format!("call {}{}", name(func), ops(n, args)),
        Rhs::Apply { clo, args } => format!("apply {}{}", n.var(*clo), ops(n, args)),
        Rhs::MkClosure { func, args } => format!("mkclosure {}{}", name(func), ops(n, args)),
        Rhs::MkThunk { func, args } => format!("mkthunk {}{}", name(func), ops(n, args)),
        Rhs::Force(v) => format!("force {}", n.var(*v)),
        Rhs::DropReuse(v) => format!("drop_reuse {}", n.var(*v)),
        Rhs::Extern { func, args } => format!("extern {}{}", name(func), ops(n, args)),
        Rhs::Global(g) => format!("global {}", name(g)),
    }
}

fn print_instr(m: &Module, n: &Names, ins: &Instr) -> String {
    match ins {
        Instr::Let { var, rhs } => {
            let v = var.map_or("_", |v| n.var(v));
            // Annotate literal types that the parser cannot infer.
            let ann = match (var, rhs) {
                (Some(_), Rhs::Lit(Const::Int(_, t))) if *t != Ty::I64 => format!(": {t}"),
                _ => String::new(),
            };
            format!("let {v}{ann} = {}", print_rhs(m, n, rhs))
        }
        Instr::LetMulti { vars, func, args } => {
            let vs: Vec<&str> = vars.iter().map(|v| n.var(*v)).collect();
            format!(
                "let ({}) = call {}{}",
                vs.join(", "),
                name(func),
                ops(n, args)
            )
        }
        Instr::Catch { vars, clo, args } => format!(
            "let ({}, {}) = catch apply {}{}",
            n.var(vars[0]),
            n.var(vars[1]),
            n.var(*clo),
            ops(n, args)
        ),
        Instr::Dup(v) => format!("dup {}", n.var(*v)),
        Instr::Drop(v) => format!("drop {}", n.var(*v)),
        Instr::Set {
            obj,
            ctor,
            field,
            val,
        } => format!(
            "set {} {} = {}",
            n.var(*obj),
            path(m, *ctor, Some(*field)),
            op(n, val)
        ),
    }
}

fn print_term(m: &Module, n: &Names, t: &Terminator) -> String {
    match t {
        Terminator::Ret(o) => format!("ret {}", op(n, o)),
        Terminator::RetMulti(os) => format!("ret {}", ops(n, os)),
        Terminator::Jump(b, args) => {
            if args.is_empty() {
                format!("jump {}", n.label(*b))
            } else {
                format!("jump {}{}", n.label(*b), ops(n, args))
            }
        }
        Terminator::Switch {
            scrut,
            arms,
            default,
        } => {
            let mut parts: Vec<String> = arms
                .iter()
                .map(|(c, b)| {
                    let c = match c {
                        Case::Ctor(c) => path(m, *c, None),
                        Case::Int(i) => i.to_string(),
                    };
                    format!("{c} => {}", n.label(*b))
                })
                .collect();
            if let Some(d) = default {
                parts.push(format!("default => {}", n.label(*d)));
            }
            format!("switch {} {{ {} }}", n.var(*scrut), parts.join(", "))
        }
        Terminator::TailCall { func, args } => {
            format!("tailcall {}{}", name(func), ops(n, args))
        }
        Terminator::Panic(s) => format!("panic {}", esc(s.as_bytes())),
        Terminator::Raise(v) => format!("raise {}", n.var(*v)),
        Terminator::Unreachable => "unreachable".into(),
    }
}
