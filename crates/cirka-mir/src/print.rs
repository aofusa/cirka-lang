//! Pretty printer for the MIR text format. `parse(print(m)) == m` holds for
//! every module produced by the parser.

use crate::ir::*;
use std::fmt::Write;

pub fn print_module(m: &Module) -> String {
    let mut s = String::new();
    for t in &m.types {
        let _ = write!(s, "type {} {{ ", t.name);
        for (i, c) in t.ctors.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            let _ = write!(s, "{} = {} {{", c.name, c.tag);
            for (j, f) in c.fields.iter().enumerate() {
                if j > 0 {
                    s.push(',');
                }
                let _ = write!(s, " {}: {}", f.name, f.ty);
            }
            s.push_str(if c.fields.is_empty() { "}" } else { " }" });
        }
        s.push_str(" }\n");
    }
    for e in &m.externs {
        let _ = write!(s, "extern fn {}(", e.name);
        for (i, (t, o)) in e.params.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            let _ = write!(s, "{t}");
            if *o == Own::Owned {
                s.push_str(" @owned");
            }
        }
        let _ = writeln!(s, ") -> {}", e.ret.map(|t| t.name()).unwrap_or("void"));
    }
    for g in &m.globals {
        let _ = writeln!(s, "global {} = {}", g.name, g.init);
    }
    for f in &m.funcs {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(&print_function(m, f));
    }
    s
}

pub fn print_function(m: &Module, f: &Function) -> String {
    let mut s = String::new();
    let _ = write!(s, "fn {}(", f.name);
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let _ = write!(
            s,
            "{}: {} @{}",
            f.var_name(p.var),
            f.ty(p.var),
            if p.own == Own::Owned {
                "owned"
            } else {
                "borrowed"
            }
        );
    }
    let _ = writeln!(s, ") -> {} {{", f.ret);
    for b in &f.blocks {
        s.push_str(&b.label);
        if !b.params.is_empty() {
            s.push('(');
            for (i, p) in b.params.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                let _ = write!(s, "{}: {}", f.var_name(*p), f.ty(*p));
            }
            s.push(')');
        }
        s.push_str(":\n");
        for ins in &b.instrs {
            s.push_str("  ");
            s.push_str(&print_instr(m, f, ins));
            s.push('\n');
        }
        s.push_str("  ");
        s.push_str(&print_term(m, f, &b.term));
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

fn op(f: &Function, o: &Operand) -> String {
    match o {
        Operand::Var(v) => f.var_name(*v).to_string(),
        Operand::Const(c) => print_const(c),
    }
}

fn ops(f: &Function, os: &[Operand]) -> String {
    let v: Vec<String> = os.iter().map(|o| op(f, o)).collect();
    format!("({})", v.join(", "))
}

pub fn print_rhs(m: &Module, f: &Function, rhs: &Rhs) -> String {
    match rhs {
        Rhs::Lit(c) => format!("lit {}", print_const(c)),
        Rhs::Prim(p, args) => format!("prim {}{}", p.name(), ops(f, args)),
        Rhs::Alloc { ctor, args, reuse } => {
            let mut s = format!("alloc {}{}", m.ctor_name(*ctor), ops(f, args));
            if let Some(r) = reuse {
                let _ = write!(s, " reuse {}", f.var_name(*r));
            }
            s
        }
        Rhs::Proj { obj, ctor, field } => format!(
            "proj {} {}.{}",
            f.var_name(*obj),
            m.ctor_name(*ctor),
            m.ctor(*ctor).fields[*field as usize].name
        ),
        Rhs::Call { func, args } => format!("call {func}{}", ops(f, args)),
        Rhs::Apply { clo, args } => format!("apply {}{}", f.var_name(*clo), ops(f, args)),
        Rhs::MkClosure { func, args } => format!("mkclosure {func}{}", ops(f, args)),
        Rhs::MkThunk { func, args } => format!("mkthunk {func}{}", ops(f, args)),
        Rhs::Force(v) => format!("force {}", f.var_name(*v)),
        Rhs::DropReuse(v) => format!("drop_reuse {}", f.var_name(*v)),
        Rhs::Extern { func, args } => format!("extern {func}{}", ops(f, args)),
        Rhs::Global(g) => format!("global {g}"),
    }
}

pub fn print_instr(m: &Module, f: &Function, ins: &Instr) -> String {
    match ins {
        Instr::Let { var, rhs } => {
            let name = var.map(|v| f.var_name(v).to_string()).unwrap_or("_".into());
            // Annotate literal types that the parser cannot infer.
            let ann = match (var, rhs) {
                (Some(_), Rhs::Lit(Const::Int(_, t))) if *t != Ty::I64 => format!(": {t}"),
                _ => String::new(),
            };
            format!("let {name}{ann} = {}", print_rhs(m, f, rhs))
        }
        Instr::Dup(v) => format!("dup {}", f.var_name(*v)),
        Instr::Drop(v) => format!("drop {}", f.var_name(*v)),
        Instr::Set {
            obj,
            ctor,
            field,
            val,
        } => format!(
            "set {} {}.{} = {}",
            f.var_name(*obj),
            m.ctor_name(*ctor),
            m.ctor(*ctor).fields[*field as usize].name,
            op(f, val)
        ),
    }
}

pub fn print_term(m: &Module, f: &Function, t: &Terminator) -> String {
    match t {
        Terminator::Ret(o) => format!("ret {}", op(f, o)),
        Terminator::Jump(b, args) => {
            if args.is_empty() {
                format!("jump {}", f.block(*b).label)
            } else {
                format!("jump {}{}", f.block(*b).label, ops(f, args))
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
                        Case::Ctor(c) => m.ctor_name(*c),
                        Case::Int(i) => i.to_string(),
                    };
                    format!("{c} => {}", f.block(*b).label)
                })
                .collect();
            if let Some(d) = default {
                parts.push(format!("default => {}", f.block(*d).label));
            }
            format!("switch {} {{ {} }}", f.var_name(*scrut), parts.join(", "))
        }
        Terminator::TailCall { func, args } => format!("tailcall {func}{}", ops(f, args)),
        Terminator::Panic(s) => format!("panic {}", esc(s.as_bytes())),
        Terminator::Unreachable => "unreachable".into(),
    }
}
