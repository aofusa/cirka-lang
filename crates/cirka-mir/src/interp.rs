//! MIR reference interpreter (06-ir.md §2.4).
//!
//! It mirrors the native runtime's semantics exactly (allocation, RC,
//! eval/apply, thunk update) so that native runs can be compared with it:
//! the program output *and* the number of allocations / frees must match.
//! It detects use after free, double free and leaks.

use crate::ir::*;
use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Val {
    I(i64),
    F(f64),
    P(u32),
    /// A null pointer (an unused reuse token).
    Null,
}

impl Val {
    fn int(self) -> i64 {
        match self {
            Val::I(i) => i,
            Val::F(f) => f.to_bits() as i64,
            Val::P(p) => p as i64 + 1,
            Val::Null => 0,
        }
    }
    fn float(self) -> f64 {
        match self {
            Val::F(f) => f,
            Val::I(i) => f64::from_bits(i as u64),
            _ => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
enum Payload {
    Con,
    Bytes(Vec<u8>),
    /// Closure / thunk code.
    Code(usize),
    Array,
}

#[derive(Clone, Debug)]
struct Obj {
    /// 0 = immortal (static).
    rc: i64,
    kind: u8,
    tag: u32,
    fields: Vec<Val>,
    payload: Payload,
    freed: bool,
    /// Size in bytes as the native runtime would allocate it.
    size: usize,
}

/// Allocation statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub allocs: u64,
    pub frees: u64,
    pub max_live: u64,
    pub reuses: u64,
    /// Executed dup/drop operations (on pointers).
    pub rc_ops: u64,
}

/// Result of running a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub stats: Stats,
    /// Objects still alive at exit (excluding statics and CAF results).
    pub leaked: u64,
}

/// A runtime error detected by the interpreter (memory safety violation).
#[derive(Debug)]
struct Fault(String);

enum Exit {
    Fault(Fault),
    Panic(String),
    Code(i32),
}

impl From<Fault> for Exit {
    fn from(f: Fault) -> Exit {
        Exit::Fault(f)
    }
}

type R<T> = Result<T, Exit>;

pub struct Interp<'m> {
    m: &'m Module,
    funcs: HashMap<&'m str, usize>,
    heap: Vec<Obj>,
    live: u64,
    stats: Stats,
    out: Vec<u8>,
    err: Vec<u8>,
    statics: HashMap<(u32, u32), u32>,
    strings: HashMap<Vec<u8>, u32>,
    globals: HashMap<String, u32>,
    /// Total executed steps (to stop runaway programs in tests).
    pub fuel: u64,
    /// Count of dup/drop operations performed (for borrow inference stats).
    pub rc_ops: u64,
    stdin: Vec<u8>,
    stdin_pos: usize,
    args: Vec<Vec<u8>>,
    /// Per function: variables that hold reuse tokens.
    tokens: Vec<Vec<bool>>,
}

const STATIC: i64 = 0;

impl<'m> Interp<'m> {
    pub fn new(m: &'m Module) -> Interp<'m> {
        Interp {
            m,
            funcs: m.func_index(),
            heap: Vec::new(),
            live: 0,
            stats: Stats::default(),
            out: Vec::new(),
            err: Vec::new(),
            statics: HashMap::new(),
            strings: HashMap::new(),
            globals: HashMap::new(),
            fuel: u64::MAX,
            rc_ops: 0,
            stdin: Vec::new(),
            stdin_pos: 0,
            args: Vec::new(),
            tokens: m.funcs.iter().map(token_vars).collect(),
        }
    }

    /// Program arguments returned by `cirka_argc` / `cirka_arg`.
    pub fn with_args(mut self, args: &[&str]) -> Self {
        self.args = args.iter().map(|a| a.as_bytes().to_vec()).collect();
        self
    }

    pub fn with_stdin(mut self, input: &[u8]) -> Self {
        self.stdin = input.to_vec();
        self
    }

    fn obj(&self, p: u32) -> R<&Obj> {
        let o = &self.heap[p as usize];
        if o.freed {
            return fault(format!("use after free of object #{p}"));
        }
        Ok(o)
    }

    fn obj_mut(&mut self, p: u32) -> R<&mut Obj> {
        if self.heap[p as usize].freed {
            return fault(format!("use after free of object #{p}"));
        }
        Ok(&mut self.heap[p as usize])
    }

    fn alloc(
        &mut self,
        kind: u8,
        tag: u32,
        fields: Vec<Val>,
        payload: Payload,
        size: usize,
    ) -> u32 {
        self.stats.allocs += 1;
        self.live += 1;
        self.stats.max_live = self.stats.max_live.max(self.live);
        self.heap.push(Obj {
            rc: 1,
            kind,
            tag,
            fields,
            payload,
            freed: false,
            size,
        });
        self.heap.len() as u32 - 1
    }

    fn static_obj(&mut self, kind: u8, tag: u32, payload: Payload) -> u32 {
        self.heap.push(Obj {
            rc: STATIC,
            kind,
            tag,
            fields: Vec::new(),
            payload,
            freed: false,
            size: 8,
        });
        self.heap.len() as u32 - 1
    }

    fn string(&mut self, s: &[u8]) -> u32 {
        if let Some(p) = self.strings.get(s) {
            return *p;
        }
        let p = self.static_obj(kind::BYTES, 0, Payload::Bytes(s.to_vec()));
        self.strings.insert(s.to_vec(), p);
        p
    }

    fn dup(&mut self, v: Val) -> R<()> {
        if let Val::P(p) = v {
            self.rc_ops += 1;
            let o = self.obj_mut(p)?;
            if o.rc != STATIC {
                o.rc += 1;
            }
        }
        Ok(())
    }

    fn drop_val(&mut self, v: Val) -> R<()> {
        let Val::P(p) = v else { return Ok(()) };
        self.rc_ops += 1;
        let mut work = vec![p];
        while let Some(p) = work.pop() {
            let o = &mut self.heap[p as usize];
            if o.freed {
                return fault(format!("double free of object #{p}"));
            }
            if o.rc == STATIC {
                continue;
            }
            o.rc -= 1;
            if o.rc > 0 {
                continue;
            }
            o.freed = true;
            self.live -= 1;
            self.stats.frees += 1;
            let fields = std::mem::take(&mut o.fields);
            for f in fields {
                if let Val::P(c) = f {
                    work.push(c);
                }
            }
        }
        Ok(())
    }

    /// Frees an object's memory without touching its fields.
    fn free_raw(&mut self, p: u32) {
        let o = &mut self.heap[p as usize];
        o.freed = true;
        o.fields.clear();
        self.live -= 1;
        self.stats.frees += 1;
    }

    /// Runs `main` (no parameters, returns the exit code as i64).
    pub fn run_main(mut self) -> Result<RunResult, String> {
        let r = match self.funcs.get("main") {
            None => return Err("no `main` function".into()),
            Some(&i) => self.call(i, vec![]),
        };
        let code = match r {
            Ok(v) => v.int() as i32,
            Err(Exit::Code(c)) => c,
            Err(Exit::Panic(msg)) => {
                self.err
                    .extend_from_slice(format!("panic: {msg}\n").as_bytes());
                1
            }
            Err(Exit::Fault(Fault(f))) => {
                return Err(format!(
                    "memory fault: {f}\nstdout so far:\n{}",
                    String::from_utf8_lossy(&self.out)
                ));
            }
        };
        let leaked = self.count_leaks();
        Ok(RunResult {
            stdout: String::from_utf8_lossy(&self.out).into_owned(),
            stderr: String::from_utf8_lossy(&self.err).into_owned(),
            exit_code: code,
            stats: Stats {
                rc_ops: self.rc_ops,
                ..self.stats
            },
            leaked,
        })
    }

    fn count_leaks(&self) -> u64 {
        // Objects reachable from globals (CAF results) are not leaks.
        let mut reach = vec![false; self.heap.len()];
        let mut work: Vec<u32> = self.globals.values().copied().collect();
        while let Some(p) = work.pop() {
            if reach[p as usize] {
                continue;
            }
            reach[p as usize] = true;
            for f in &self.heap[p as usize].fields {
                if let Val::P(c) = f {
                    work.push(*c);
                }
            }
        }
        self.heap
            .iter()
            .enumerate()
            .filter(|(i, o)| !o.freed && o.rc != STATIC && !reach[*i])
            .count() as u64
    }

    fn operand(&mut self, frame: &[Val], o: &Operand) -> Val {
        match o {
            Operand::Var(v) => frame[v.0 as usize],
            Operand::Const(Const::Int(i, _)) => Val::I(*i),
            Operand::Const(Const::Float(f)) => Val::F(*f),
            Operand::Const(Const::Str(s)) => Val::P(self.string(s)),
        }
    }

    fn operands(&mut self, frame: &[Val], os: &[Operand]) -> Vec<Val> {
        os.iter().map(|o| self.operand(frame, o)).collect()
    }

    fn call(&mut self, fi: usize, args: Vec<Val>) -> R<Val> {
        let m = self.m;
        let mut cur = fi;
        let mut f = &m.funcs[fi];
        let mut frame = vec![Val::I(0); f.var_names.len()];
        for (p, a) in f.params.iter().zip(args) {
            frame[p.var.0 as usize] = a;
        }
        let mut bb = 0usize;
        loop {
            let block = &f.blocks[bb];
            let n = block.instrs.len();
            let mut tail: Option<(usize, Vec<Val>)> = None;
            for (ii, ins) in block.instrs.iter().enumerate() {
                if self.fuel == 0 {
                    return Err(Exit::Panic("interpreter fuel exhausted".into()));
                }
                self.fuel -= 1;
                // `let r = call/apply ...; ret r` is a tail call.
                if ii + 1 == n
                    && let (Instr::Let { var: Some(r), rhs }, Terminator::Ret(Operand::Var(rv))) =
                        (ins, &block.term)
                    && r == rv
                {
                    match rhs {
                        Rhs::Call { func, args } => {
                            let vals = self.operands(&frame, args);
                            match self.funcs.get(func.as_str()) {
                                Some(&gi) => tail = Some((gi, vals)),
                                None => return fault(format!("unknown function {func}")),
                            }
                            break;
                        }
                        Rhs::Apply { clo, args } => {
                            let c = frame[clo.0 as usize];
                            let vals = self.operands(&frame, args);
                            match self.apply_step(c, vals)? {
                                Step::Value(v) => return Ok(v),
                                Step::Call(gi, all, None) => tail = Some((gi, all)),
                                Step::Call(gi, all, Some(p)) => {
                                    let r = self.call(gi, all)?;
                                    self.drop_val(Val::P(p))?;
                                    return Ok(r);
                                }
                            }
                            break;
                        }
                        _ => {}
                    }
                }
                self.instr(cur, f, &mut frame, ins)?;
            }
            if let Some((gi, vals)) = tail {
                cur = gi;
                f = &m.funcs[gi];
                frame = vec![Val::I(0); f.var_names.len()];
                for (p, a) in f.params.iter().zip(vals) {
                    frame[p.var.0 as usize] = a;
                }
                bb = 0;
                continue;
            }
            match &block.term {
                Terminator::Ret(o) => return Ok(self.operand(&frame, o)),
                Terminator::Jump(b, args) => {
                    let vals = self.operands(&frame, args);
                    let target = &f.blocks[b.0 as usize];
                    for (p, v) in target.params.iter().zip(vals) {
                        frame[p.0 as usize] = v;
                    }
                    bb = b.0 as usize;
                }
                Terminator::Switch {
                    scrut,
                    arms,
                    default,
                } => {
                    let v = frame[scrut.0 as usize];
                    let mut next = *default;
                    match v {
                        Val::P(p) => {
                            let tag = {
                                let o = self.obj(p)?;
                                if o.kind != kind::CON {
                                    return fault(format!(
                                        "switch on a non-constructor object (kind {})",
                                        o.kind
                                    ));
                                }
                                o.tag
                            };
                            for (c, b) in arms {
                                if let Case::Ctor(c) = c
                                    && m.ctor(*c).tag == tag
                                {
                                    next = Some(*b);
                                    break;
                                }
                            }
                        }
                        v => {
                            let i = v.int();
                            for (c, b) in arms {
                                if let Case::Int(k) = c
                                    && *k == i
                                {
                                    next = Some(*b);
                                    break;
                                }
                            }
                        }
                    }
                    match next {
                        Some(b) => bb = b.0 as usize,
                        None => return Err(Exit::Panic("no matching switch arm".into())),
                    }
                }
                Terminator::TailCall { func, args } => {
                    let vals = self.operands(&frame, args);
                    let Some(&gi) = self.funcs.get(func.as_str()) else {
                        return fault(format!("unknown function {func}"));
                    };
                    cur = gi;
                    f = &m.funcs[gi];
                    frame = vec![Val::I(0); f.var_names.len()];
                    for (p, a) in f.params.iter().zip(vals) {
                        frame[p.var.0 as usize] = a;
                    }
                    bb = 0;
                }
                Terminator::Panic(msg) => return Err(Exit::Panic(msg.clone())),
                Terminator::Unreachable => return fault("reached `unreachable`"),
            }
        }
    }

    fn call_named(&mut self, name: &str, args: Vec<Val>) -> R<Val> {
        match self.funcs.get(name) {
            Some(&i) => self.call(i, args),
            None => fault(format!("unknown function {name}")),
        }
    }

    fn instr(&mut self, fi: usize, f: &Function, frame: &mut [Val], ins: &Instr) -> R<()> {
        match ins {
            Instr::Dup(v) => self.dup(frame[v.0 as usize]),
            Instr::Drop(v) => {
                let val = frame[v.0 as usize];
                if self.tokens[fi][v.0 as usize] {
                    if let Val::P(p) = val {
                        self.obj(p)?;
                        self.free_raw(p);
                    }
                    Ok(())
                } else {
                    self.drop_val(val)
                }
            }
            Instr::Set {
                obj,
                ctor: _,
                field,
                val,
            } => {
                let v = self.operand(frame, val);
                let Val::P(p) = frame[obj.0 as usize] else {
                    return fault("set on a non-pointer");
                };
                let o = self.obj_mut(p)?;
                if o.rc != 1 {
                    return fault(format!("set on a shared object (rc={})", o.rc));
                }
                o.fields[*field as usize] = v;
                Ok(())
            }
            Instr::Let { var, rhs } => {
                let v = self.rhs(f, frame, rhs)?;
                if let Some(x) = var {
                    frame[x.0 as usize] = v;
                }
                Ok(())
            }
        }
    }

    fn rhs(&mut self, _f: &Function, frame: &mut [Val], rhs: &Rhs) -> R<Val> {
        let m = self.m;
        Ok(match rhs {
            Rhs::Lit(c) => self.operand(frame, &Operand::Const(c.clone())),
            Rhs::Prim(p, args) => {
                let a = self.operands(frame, args);
                self.prim(*p, &a)?
            }
            Rhs::Alloc { ctor, args, reuse } => {
                let def = m.ctor(*ctor);
                let vals = self.operands(frame, args);
                if def.fields.is_empty() {
                    let key = (ctor.ty, ctor.ctor);
                    let p = match self.statics.get(&key) {
                        Some(p) => *p,
                        None => {
                            let p = self.static_obj(kind::CON, def.tag, Payload::Con);
                            self.statics.insert(key, p);
                            p
                        }
                    };
                    return Ok(Val::P(p));
                }
                if let Some(tok) = reuse
                    && let Val::P(t) = frame[tok.0 as usize]
                {
                    let size = def.size();
                    let o = self.obj_mut(t)?;
                    if o.size != size {
                        return fault("reuse token size mismatch");
                    }
                    o.rc = 1;
                    o.kind = kind::CON;
                    o.tag = def.tag;
                    o.fields = vals;
                    o.payload = Payload::Con;
                    self.stats.reuses += 1;
                    return Ok(Val::P(t));
                }
                Val::P(self.alloc(kind::CON, def.tag, vals, Payload::Con, def.size()))
            }
            Rhs::Proj { obj, ctor, field } => {
                let Val::P(p) = frame[obj.0 as usize] else {
                    return fault("proj on a non-pointer");
                };
                let o = self.obj(p)?;
                let tag = m.ctor(*ctor).tag;
                if o.kind != kind::CON || o.tag != tag {
                    return fault(format!(
                        "proj {} on an object with tag {}",
                        m.ctor_name(*ctor),
                        o.tag
                    ));
                }
                o.fields[*field as usize]
            }
            Rhs::Call { func, args } => {
                let vals = self.operands(frame, args);
                self.call_named(func, vals)?
            }
            Rhs::Apply { clo, args } => {
                let c = frame[clo.0 as usize];
                let vals = self.operands(frame, args);
                self.apply(c, vals)?
            }
            Rhs::MkClosure { func, args } => {
                let vals = self.operands(frame, args);
                let Some(&gi) = self.funcs.get(func.as_str()) else {
                    return fault(format!("unknown function {func}"));
                };
                let arity = m.funcs[gi].params.len() - vals.len();
                let size = 16 + 8 * vals.len();
                Val::P(self.alloc(kind::FUN, arity as u32, vals, Payload::Code(gi), size))
            }
            Rhs::MkThunk { func, args } => {
                let vals = self.operands(frame, args);
                let Some(&gi) = self.funcs.get(func.as_str()) else {
                    return fault(format!("unknown function {func}"));
                };
                let size = (16 + 8 * vals.len()).max(16);
                Val::P(self.alloc(kind::THUNK, 0, vals, Payload::Code(gi), size))
            }
            Rhs::Force(v) => {
                let val = frame[v.0 as usize];
                self.force(val)?
            }
            Rhs::DropReuse(v) => {
                let Val::P(p) = frame[v.0 as usize] else {
                    return Ok(Val::Null);
                };
                let o = self.obj(p)?;
                if o.rc == 1 {
                    let fields = std::mem::take(&mut self.heap[p as usize].fields);
                    for fv in fields {
                        self.drop_val(fv)?;
                    }
                    self.rc_ops += 1;
                    Val::P(p)
                } else {
                    self.drop_val(Val::P(p))?;
                    Val::Null
                }
            }
            Rhs::Extern { func, args } => {
                let vals = self.operands(frame, args);
                self.external(func, &vals)?
            }
            Rhs::Global(g) => {
                if let Some(p) = self.globals.get(g) {
                    return Ok(Val::P(*p));
                }
                let Some(decl) = m.globals.iter().find(|x| &x.name == g) else {
                    return fault(format!("unknown global {g}"));
                };
                let Some(&gi) = self.funcs.get(decl.init.as_str()) else {
                    return fault(format!("unknown function {}", decl.init));
                };
                let p = self.static_obj(kind::THUNK, 0, Payload::Code(gi));
                self.globals.insert(g.clone(), p);
                Val::P(p)
            }
        })
    }

    /// `force`: consumes the reference and returns an owned value.
    fn force(&mut self, v: Val) -> R<Val> {
        let Val::P(p) = v else { return Ok(v) };
        let (k, rc) = {
            let o = self.obj(p)?;
            (o.kind, o.rc)
        };
        match k {
            kind::IND => {
                let r = self.heap[p as usize].fields[0];
                self.dup(r)?;
                self.drop_val(Val::P(p))?;
                Ok(r)
            }
            kind::BLACKHOLE => Err(Exit::Panic("<<loop>>".into())),
            kind::THUNK => {
                let Payload::Code(gi) = self.heap[p as usize].payload else {
                    return fault("thunk without code");
                };
                self.heap[p as usize].kind = kind::BLACKHOLE;
                let captured = self.heap[p as usize].fields.clone();
                // The thunk entry stub passes captured values: owned
                // parameters get a fresh reference.
                let g = &self.m.funcs[gi];
                for (i, c) in captured.iter().enumerate() {
                    if g.params[i].own == Own::Owned {
                        self.dup(*c)?;
                    }
                }
                let r = self.call(gi, captured.clone())?;
                // Update: drop captured values, become an indirection.
                let o = &mut self.heap[p as usize];
                let old = std::mem::replace(&mut o.fields, vec![r]);
                o.kind = kind::IND;
                for c in old {
                    self.drop_val(c)?;
                }
                if rc == STATIC {
                    self.dup(r)?;
                    return Ok(r);
                }
                self.dup(r)?;
                self.drop_val(Val::P(p))?;
                Ok(r)
            }
            _ => Ok(v),
        }
    }

    /// Generic eval/apply (05-runtime.md §4). Consumes `f` and `args`.
    fn apply(&mut self, f: Val, args: Vec<Val>) -> R<Val> {
        match self.apply_step(f, args)? {
            Step::Value(v) => Ok(v),
            Step::Call(gi, all, post) => {
                let r = self.call(gi, all)?;
                if let Some(p) = post {
                    self.drop_val(Val::P(p))?;
                }
                Ok(r)
            }
        }
    }

    /// One step of eval/apply: either a value, or the final (tail) call of
    /// a closure's code, so that callers in tail position can reuse their
    /// frame (mirrors `musttail` in the native code).
    fn apply_step(&mut self, f: Val, args: Vec<Val>) -> R<Step> {
        let Val::P(p) = f else {
            return fault("apply of a non-pointer");
        };
        let (k, arity) = {
            let o = self.obj(p)?;
            (o.kind, o.tag as usize)
        };
        match k {
            kind::FUN => {
                let n = args.len();
                if n == arity {
                    self.enter_closure(p, args)
                } else if n < arity {
                    let mut fields = vec![Val::P(p)];
                    fields.extend(args);
                    let size = 8 + 8 * fields.len();
                    let tag = n as u32;
                    Ok(Step::Value(Val::P(self.alloc(
                        kind::PAP,
                        tag,
                        fields,
                        Payload::Con,
                        size,
                    ))))
                } else {
                    let rest = args[arity..].to_vec();
                    let first = args[..arity].to_vec();
                    let r = match self.enter_closure(p, first)? {
                        Step::Value(v) => v,
                        Step::Call(gi, all, post) => {
                            let r = self.call(gi, all)?;
                            if let Some(p) = post {
                                self.drop_val(Val::P(p))?;
                            }
                            r
                        }
                    };
                    self.apply_step(r, rest)
                }
            }
            kind::PAP => {
                let fields = self.heap[p as usize].fields.clone();
                let g = fields[0];
                let m = fields.len() - 1;
                let g_arity = match g {
                    Val::P(q) => self.obj(q)?.tag as usize,
                    _ => return fault("PAP without a function"),
                };
                let rem = g_arity.saturating_sub(m);
                if rem >= 1 && args.len() > rem {
                    // Over-application of a PAP: saturate first, then apply
                    // the result to the remaining arguments.
                    let first = args[..rem].to_vec();
                    let rest = args[rem..].to_vec();
                    let r = self.apply(f, first)?;
                    return self.apply_step(r, rest);
                }
                if self.heap[p as usize].rc == 1 {
                    self.free_raw(p);
                } else {
                    for v in &fields {
                        self.dup(*v)?;
                    }
                    self.drop_val(Val::P(p))?;
                }
                let mut all = fields[1..].to_vec();
                all.extend(args);
                self.apply_step(g, all)
            }
            kind::THUNK | kind::IND => {
                let v = self.force(f)?;
                self.apply_step(v, args)
            }
            _ => fault(format!("apply of a non-function object (kind {k})")),
        }
    }

    /// The closure entry stub: moves (or dups) captured values out of the
    /// closure and calls the code with captured ++ args.
    fn enter_closure(&mut self, p: u32, args: Vec<Val>) -> R<Step> {
        let Payload::Code(gi) = self.heap[p as usize].payload else {
            return fault("closure without code");
        };
        let captured = self.heap[p as usize].fields.clone();
        let g = &self.m.funcs[gi];
        let unique = self.heap[p as usize].rc == 1;
        // Borrowed captured parameters must stay alive during the call.
        let any_borrowed = g.params[..captured.len()]
            .iter()
            .any(|q| q.own == Own::Borrowed);
        if unique && !any_borrowed {
            self.free_raw(p);
        } else {
            for (i, c) in captured.iter().enumerate() {
                if g.params[i].own == Own::Owned {
                    self.dup(*c)?;
                }
            }
            if !any_borrowed {
                self.drop_val(Val::P(p))?;
            }
        }
        let mut all = captured;
        all.extend(args);
        Ok(Step::Call(
            gi,
            all,
            if any_borrowed { Some(p) } else { None },
        ))
    }

    fn bytes(&self, v: Val) -> R<&Vec<u8>> {
        let Val::P(p) = v else {
            return fault("expected a bytes object");
        };
        match &self.obj(p)?.payload {
            Payload::Bytes(b) => Ok(b),
            _ => fault("expected a bytes object"),
        }
    }

    fn prim(&mut self, p: Prim, a: &[Val]) -> R<Val> {
        use Prim::*;
        let i = |k: usize| a[k].int();
        let fl = |k: usize| a[k].float();
        let b = |x: bool| Val::I(x as i64);
        Ok(match p {
            Add => Val::I(i(0).wrapping_add(i(1))),
            Sub => Val::I(i(0).wrapping_sub(i(1))),
            Mul => Val::I(i(0).wrapping_mul(i(1))),
            Div => {
                if i(1) == 0 {
                    return Err(Exit::Panic("divide by zero".into()));
                }
                Val::I(i(0).wrapping_div(i(1)))
            }
            Rem => {
                if i(1) == 0 {
                    return Err(Exit::Panic("divide by zero".into()));
                }
                Val::I(i(0).wrapping_rem(i(1)))
            }
            Neg => Val::I(i(0).wrapping_neg()),
            And => Val::I(i(0) & i(1)),
            Or => Val::I(i(0) | i(1)),
            Xor => Val::I(i(0) ^ i(1)),
            Shl => Val::I(i(0).wrapping_shl(i(1) as u32)),
            Shr => Val::I(i(0).wrapping_shr(i(1) as u32)),
            LShr => Val::I(((i(0) as u64).wrapping_shr(i(1) as u32)) as i64),
            Eq => b(i(0) == i(1)),
            Ne => b(i(0) != i(1)),
            Lt => b(i(0) < i(1)),
            Le => b(i(0) <= i(1)),
            Gt => b(i(0) > i(1)),
            Ge => b(i(0) >= i(1)),
            FAdd => Val::F(fl(0) + fl(1)),
            FSub => Val::F(fl(0) - fl(1)),
            FMul => Val::F(fl(0) * fl(1)),
            FDiv => Val::F(fl(0) / fl(1)),
            FNeg => Val::F(-fl(0)),
            FEq => b(fl(0) == fl(1)),
            FNe => b(fl(0) != fl(1)),
            FLt => b(fl(0) < fl(1)),
            FLe => b(fl(0) <= fl(1)),
            FGt => b(fl(0) > fl(1)),
            FGe => b(fl(0) >= fl(1)),
            I64ToF64 => Val::F(i(0) as f64),
            F64ToI64 => Val::I(fl(0) as i64),
            I64ToI32 => Val::I(i(0) as i32 as i64),
            I32ToI64 => Val::I(i(0) as i32 as i64),
            I64ToI8 => Val::I(i(0) as i8 as i64),
            I8ToI64 => Val::I(i(0) as i8 as i64),
            F64Bits => Val::I(fl(0).to_bits() as i64),
            BitsF64 => Val::F(f64::from_bits(i(0) as u64)),
            PtrEq => b(a[0] == a[1]),
            IsUnique => match a[0] {
                Val::P(p) => b(self.obj(p)?.rc == 1),
                _ => b(false),
            },
            BytesNew => {
                let n = i(0).max(0) as usize;
                let size = 16 + n.div_ceil(8) * 8;
                Val::P(self.alloc(kind::BYTES, 0, vec![], Payload::Bytes(vec![0; n]), size))
            }
            BytesLen => Val::I(self.bytes(a[0])?.len() as i64),
            BytesGet => {
                let bs = self.bytes(a[0])?;
                let k = i(1);
                if k < 0 || k as usize >= bs.len() {
                    return Err(Exit::Panic("index out of bounds".into()));
                }
                Val::I(bs[k as usize] as i64)
            }
            BytesSet => {
                let Val::P(p) = a[0] else {
                    return fault("bytes_set on a non-pointer");
                };
                let k = i(1);
                let v = i(2) as u8;
                let o = self.obj_mut(p)?;
                if o.rc != 1 {
                    return fault("bytes_set on a shared object");
                }
                match &mut o.payload {
                    Payload::Bytes(bs) if k >= 0 && (k as usize) < bs.len() => bs[k as usize] = v,
                    Payload::Bytes(_) => return Err(Exit::Panic("index out of bounds".into())),
                    _ => return fault("bytes_set on a non-bytes object"),
                }
                Val::I(0)
            }
            BytesData => Val::I(0),
            ArrayNew => {
                let n = i(0).max(0) as usize;
                let init = a[1];
                if n == 0 {
                    self.drop_val(init)?;
                } else {
                    for _ in 1..n {
                        self.dup(init)?;
                    }
                }
                Val::P(self.alloc(kind::ARRAY, 0, vec![init; n], Payload::Array, 16 + 8 * n))
            }
            ArrayLen => {
                let Val::P(p) = a[0] else {
                    return fault("array_len on a non-pointer");
                };
                Val::I(self.obj(p)?.fields.len() as i64)
            }
            ArrayGet => {
                let Val::P(p) = a[0] else {
                    return fault("array_get on a non-pointer");
                };
                let o = self.obj(p)?;
                let k = i(1);
                if k < 0 || k as usize >= o.fields.len() {
                    return Err(Exit::Panic("index out of bounds".into()));
                }
                o.fields[k as usize]
            }
            ArraySet => {
                let Val::P(p) = a[0] else {
                    return fault("array_set on a non-pointer");
                };
                let k = i(1);
                let o = self.obj_mut(p)?;
                if o.rc != 1 {
                    return fault("array_set on a shared object");
                }
                if k < 0 || k as usize >= o.fields.len() {
                    return Err(Exit::Panic("index out of bounds".into()));
                }
                let old = std::mem::replace(&mut o.fields[k as usize], a[2]);
                self.drop_val(old)?;
                Val::I(0)
            }
        })
    }

    /// Runtime functions callable through `extern` in the interpreter.
    fn external(&mut self, name: &str, a: &[Val]) -> R<Val> {
        let unit = Val::I(0);
        match name {
            "cirka_write_i64" => {
                let _ = write!(StrW(&mut self.out), "{}", a[0].int());
                Ok(unit)
            }
            "cirka_write_f64" => {
                let s = crate::interp::format_f64(a[0].float());
                self.out.extend_from_slice(s.as_bytes());
                Ok(unit)
            }
            "cirka_write_char" => {
                let c = char::from_u32(a[0].int() as u32).unwrap_or('\u{fffd}');
                let mut buf = [0u8; 4];
                self.out
                    .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                Ok(unit)
            }
            "cirka_write_bytes" => {
                let b = self.bytes(a[0])?.clone();
                self.out.extend_from_slice(&b);
                Ok(unit)
            }
            "cirka_ewrite_bytes" => {
                let b = self.bytes(a[0])?.clone();
                self.err.extend_from_slice(&b);
                Ok(unit)
            }
            "cirka_f64_to_bytes" => {
                let b = crate::interp::format_f64(a[0].float()).into_bytes();
                let size = 16 + b.len().div_ceil(8) * 8;
                Ok(Val::P(self.alloc(
                    kind::BYTES,
                    0,
                    vec![],
                    Payload::Bytes(b),
                    size,
                )))
            }
            "sqrt" => Ok(Val::F(a[0].float().sqrt())),
            "exp" => Ok(Val::F(a[0].float().exp())),
            "log" => Ok(Val::F(a[0].float().ln())),
            "sin" => Ok(Val::F(a[0].float().sin())),
            "cos" => Ok(Val::F(a[0].float().cos())),
            "tan" => Ok(Val::F(a[0].float().tan())),
            "asin" => Ok(Val::F(a[0].float().asin())),
            "acos" => Ok(Val::F(a[0].float().acos())),
            "atan" => Ok(Val::F(a[0].float().atan())),
            "sinh" => Ok(Val::F(a[0].float().sinh())),
            "cosh" => Ok(Val::F(a[0].float().cosh())),
            "tanh" => Ok(Val::F(a[0].float().tanh())),
            "atan2" => Ok(Val::F(a[0].float().atan2(a[1].float()))),
            "pow" => Ok(Val::F(a[0].float().powf(a[1].float()))),
            "cirka_f64_show_hs" => {
                let b = show_hs(a[0].float()).into_bytes();
                let size = 16 + b.len().div_ceil(8) * 8;
                Ok(Val::P(self.alloc(
                    kind::BYTES,
                    0,
                    vec![],
                    Payload::Bytes(b),
                    size,
                )))
            }
            "cirka_parse_f64" => {
                let b = self.bytes(a[0])?.clone();
                Ok(Val::F(
                    std::str::from_utf8(&b)
                        .ok()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(f64::NAN),
                ))
            }
            "cirka_flush" => Ok(unit),
            "cirka_exit" => Err(Exit::Code(a[0].int() as i32)),
            "cirka_read_byte" => {
                let r = match self.stdin.get(self.stdin_pos) {
                    Some(b) => {
                        self.stdin_pos += 1;
                        *b as i64
                    }
                    None => -1,
                };
                Ok(Val::I(r))
            }
            "cirka_argc" => Ok(Val::I(self.args.len() as i64)),
            "cirka_arg" => {
                let k = a[0].int();
                let Some(b) = usize::try_from(k)
                    .ok()
                    .and_then(|k| self.args.get(k))
                    .cloned()
                else {
                    return Err(Exit::Panic("argument index out of range".into()));
                };
                let size = 16 + b.len().div_ceil(8) * 8;
                Ok(Val::P(self.alloc(
                    kind::BYTES,
                    0,
                    vec![],
                    Payload::Bytes(b),
                    size,
                )))
            }
            "cirka_panic_bytes" => {
                let b = self.bytes(a[0])?.clone();
                Err(Exit::Panic(String::from_utf8_lossy(&b).into_owned()))
            }
            _ => fault(format!(
                "extern `{name}` is not available in the interpreter"
            )),
        }
    }
}

enum Step {
    Value(Val),
    /// Call function `.0` with `.1`; drop object `.2` afterwards.
    Call(usize, Vec<Val>, Option<u32>),
}

fn fault<T>(msg: impl Into<String>) -> R<T> {
    Err(Exit::Fault(Fault(msg.into())))
}

struct StrW<'a>(&'a mut Vec<u8>);
impl std::fmt::Write for StrW<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// Formatting of `cirka_write_f64` (shared with the runtime: Rust's
/// shortest round-trip `Display`).
/// `show` of a `Double` in Haskell's format (same as the runtime's
/// `cirka_f64_show_hs`).
pub fn show_hs(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}Infinity");
    }
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    let s = format!("{:e}", x.abs());
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let digits: String = m.chars().filter(|c| c.is_ascii_digit()).collect();
    let e: i32 = e.parse::<i32>().unwrap_or(0) + 1;
    if (0..=7).contains(&e) {
        let e = e as usize;
        if e == 0 {
            return format!("{sign}0.{digits}");
        }
        let int: String = (0..e)
            .map(|i| digits.as_bytes().get(i).map(|b| *b as char).unwrap_or('0'))
            .collect();
        let frac = if digits.len() > e { &digits[e..] } else { "0" };
        format!("{sign}{int}.{frac}")
    } else {
        let rest = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{sign}{}.{rest}e{}", &digits[..1], e - 1)
    }
}

pub fn format_f64(x: f64) -> String {
    format!("{x}")
}

fn token_vars(f: &Function) -> Vec<bool> {
    let mut t = vec![false; f.var_names.len()];
    for b in &f.blocks {
        for i in &b.instrs {
            if let Instr::Let {
                var: Some(x),
                rhs: Rhs::DropReuse(_),
            } = i
            {
                t[x.0 as usize] = true;
            }
        }
    }
    t
}

/// Runs `main` on a thread with a large stack (deep non-tail recursion).
pub fn run(m: &Module, stdin: &[u8], fuel: u64) -> Result<RunResult, String> {
    run_with_args(m, &[], stdin, fuel)
}

/// Like [`run`], with program arguments.
pub fn run_with_args(
    m: &Module,
    args: &[&str],
    stdin: &[u8],
    fuel: u64,
) -> Result<RunResult, String> {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(1 << 30)
            .spawn_scoped(s, || {
                let mut it = Interp::new(m).with_stdin(stdin).with_args(args);
                it.fuel = fuel;
                it.run_main()
            })
            .expect("spawn interpreter thread")
            .join()
            .unwrap_or_else(|_| Err("interpreter panicked".into()))
    })
}
