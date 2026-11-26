//! MIR verifier (06-ir.md §2.3): types, scopes and ownership.
//!
//! Ownership is checked by a forward data-flow analysis over basic blocks.
//! Every pointer variable carries an *owned count* (how many references the
//! function currently owns through it). Borrowed variables (borrowed
//! parameters, `proj` / `array_get` results) are usable while their source
//! is alive; consuming them requires a preceding `dup`.
//!
//! - owned references must be consumed exactly once on every path
//!   (`ret`, owned call argument, constructor field, `drop`, ...),
//! - consumed variables must not be used again,
//! - borrowed variables must not be consumed without `dup`,
//! - reuse tokens are used by at most one `alloc` of the same size.
//!
//! Static objects ([`Function::static_vars`]: constructors without fields,
//! string constants and block parameters that only receive them) are
//! immortal: like constants, they may be used and consumed any number of
//! times, and need no `drop` (FEAT-0089).

use crate::ir::*;
use cirka_diagnostics::Diagnostic;
use std::collections::{BTreeMap, HashMap};

/// Verifies a whole module; returns all errors found.
pub fn verify_module(m: &Module) -> Vec<Diagnostic> {
    let mut errs = Vec::new();
    let sigs: HashMap<&str, &Function> = m.funcs.iter().map(|f| (f.name.as_str(), f)).collect();
    if sigs.len() != m.funcs.len() {
        errs.push(Diagnostic::error("duplicate function names").with_code("M0100"));
    }
    for g in &m.globals {
        match sigs.get(g.init.as_str()) {
            Some(f) if f.params.is_empty() && f.results() == [Ty::Ptr] => {}
            _ => errs.push(
                Diagnostic::error(format!(
                    "global `{}`: initializer `{}` must be a function with no parameters returning ptr",
                    g.name, g.init
                ))
                .with_code("M0101"),
            ),
        }
    }
    for e in &m.externs {
        if let Err(msg) = extern_shape(m, e, &sigs) {
            errs.push(Diagnostic::error(format!("extern `{}`: {msg}", e.name)).with_code("M0102"));
        }
    }
    for (what, msg) in m.ctypes().check_decls() {
        errs.push(Diagnostic::error(format!("{what}: {msg}")).with_code("M0102"));
    }
    let uses: Vec<cirka_core::ctype::LinkUse<'_>> = m
        .externs
        .iter()
        .map(|e| cirka_core::ctype::LinkUse {
            name: &e.name,
            symbol: e.symbol(),
            kind: &e.kind,
            sig: e.c_sig.as_ref(),
        })
        .collect();
    for (what, msg) in cirka_core::ctype::check_links(&uses) {
        errs.push(Diagnostic::error(format!("{what}: {msg}")).with_code("M0102"));
    }
    let unwinding = m.catches();
    for f in &m.funcs {
        let mut v = FnVerifier::new(m, f, &sigs);
        v.unwinding = unwinding;
        v.run();
        errs.extend(v.errs);
    }
    errs
}

/// The owned references `f` (a verified function of `m`) holds at each
/// instruction marked in `points` (by block and instruction): before the
/// instruction for [`Point::Before`], after it took its operands and before
/// it defines its results for [`Point::After`] (`unwind::cleanups`).
///
/// [`Point::Before`]: crate::unwind::Point::Before
/// [`Point::After`]: crate::unwind::Point::After
pub fn owned_at(
    m: &Module,
    f: &Function,
    points: &[Vec<crate::unwind::Point>],
) -> crate::unwind::Cleanups {
    let sigs: HashMap<&str, &Function> = m.funcs.iter().map(|g| (g.name.as_str(), g)).collect();
    let mut v = FnVerifier::new(m, f, &sigs);
    v.points = points;
    v.unwinding = true;
    v.run();
    v.held
}

/// The MIR types of an extern agree with its C signature and kind
/// (08-ffi.md §7.2): `i64` for integers, pointers and the addresses of
/// aggregates, `f64` for floating point, no result for `void` and for an
/// aggregate result (written to the address before the arguments).
fn extern_shape(m: &Module, e: &ExternDecl, sigs: &HashMap<&str, &Function>) -> Result<(), String> {
    use cirka_core::{CTy, CType, ExternKind};
    let rep = |c: &CType| match c {
        CType::Scalar(s) if s.is_float() => Ty::F64,
        _ => Ty::I64,
    };
    if e.link.is_some() && e.c_sig.is_none() && e.kind != ExternKind::Addr {
        return Err("a link name needs a C signature".into());
    }
    let sig = match (&e.kind, &e.c_sig) {
        (ExternKind::Call, None) => return Ok(()),
        (ExternKind::Addr, sig) => {
            if let Some(s) = sig {
                m.ctypes().check_sig(s, &e.kind)?;
            }
            if !e.params.is_empty() || e.ret != Some(Ty::I64) {
                return Err("an address takes no parameters and is i64".into());
            }
            return Ok(());
        }
        (_, None) => return Err("needs a C signature".into()),
        (_, Some(s)) => s,
    };
    m.ctypes().check_sig(sig, &e.kind)?;
    let agg_ret = sig.ret.is_agg();
    let ret = match &sig.ret {
        CType::Scalar(CTy::Void) => None,
        _ if agg_ret => None,
        r => Some(rep(r)),
    };
    let mut want = Vec::new();
    if e.kind == ExternKind::Dynamic {
        want.push(Ty::I64);
    }
    if agg_ret {
        want.push(Ty::I64);
    }
    want.extend(sig.params.iter().map(rep));
    if let ExternKind::Callback(h) = &e.kind {
        if e.params != [(Ty::Ptr, Own::Owned)] || e.ret != Some(Ty::I64) {
            return Err("a callback takes its environment (ptr @owned) and is i64".into());
        }
        let Some(f) = sigs.get(h.as_str()) else {
            return Err(format!("unknown handler `{h}`"));
        };
        let mut params = vec![Ty::Ptr];
        params.extend(want);
        let res = ret.unwrap_or(Ty::I64);
        if f.param_tys() != params || f.results() != [res] {
            return Err(format!(
                "handler `{h}` must take ({}) and return {res}",
                params
                    .iter()
                    .map(|t| t.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        return Ok(());
    }
    let have: Vec<Ty> = e.params.iter().map(|p| p.0).collect();
    if have != want || e.ret != ret {
        return Err("does not match its C signature".into());
    }
    Ok(())
}

/// `i64` or `(ptr, i64)`.
fn show_tys(tys: &[Ty]) -> String {
    match tys {
        [t] => t.to_string(),
        _ => {
            let v: Vec<&str> = tys.iter().map(|t| t.name()).collect();
            format!("({})", v.join(", "))
        }
    }
}

/// Renders verifier errors to a single string (for tests and ICE reports).
pub fn render_errors(errs: &[Diagnostic]) -> String {
    let sm = cirka_diagnostics::SourceMap::new();
    errs.iter()
        .map(|d| cirka_diagnostics::render(d, &sm, cirka_diagnostics::Style::Plain))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    /// Defined variables and their owned counts.
    vars: BTreeMap<Var, u32>,
    /// Variables known to hold a given constructor.
    known: BTreeMap<Var, CtorRef>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Origin {
    Scalar,
    /// Owned pointer (params @owned, results of allocating instructions).
    Owned,
    /// Borrowed parameter: alive for the whole function.
    BorrowedParam,
    /// Borrowed from another variable (`proj`, `array_get`).
    BorrowedFrom(Var),
    /// Reuse token from `drop_reuse`.
    Token,
    /// Static immortal object (no reference count).
    Static,
}

struct FnVerifier<'a> {
    m: &'a Module,
    f: &'a Function,
    sigs: &'a HashMap<&'a str, &'a Function>,
    origin: Vec<Origin>,
    /// Size in bytes of a reuse token (0 = unknown).
    token_size: HashMap<Var, usize>,
    errs: Vec<Diagnostic>,
    cur_block: usize,
    /// The raising positions whose ownership state to record (FEAT-0040,
    /// `unwind::cleanups`), by block and instruction; empty when only
    /// verifying.
    points: &'a [Vec<crate::unwind::Point>],
    cur_instr: usize,
    held: crate::unwind::Cleanups,
    /// Whether the program unwinds exceptions (`Module::catches`): `panic`
    /// then raises, and must not hold anything either (06-ir.md §2.5).
    unwinding: bool,
}

impl<'a> FnVerifier<'a> {
    fn new(m: &'a Module, f: &'a Function, sigs: &'a HashMap<&'a str, &'a Function>) -> Self {
        let mut origin = vec![Origin::Scalar; f.var_names.len()];
        for p in &f.params {
            if f.ty(p.var).is_ptr() {
                origin[p.var.0 as usize] = match p.own {
                    Own::Owned => Origin::Owned,
                    Own::Borrowed => Origin::BorrowedParam,
                };
            }
        }
        for b in &f.blocks {
            for p in &b.params {
                if f.ty(*p).is_ptr() {
                    origin[p.0 as usize] = Origin::Owned;
                }
            }
            for ins in &b.instrs {
                if let Instr::Let { var: Some(v), rhs } = ins
                    && f.ty(*v).is_ptr()
                {
                    origin[v.0 as usize] = match rhs {
                        Rhs::Proj { obj, .. } => Origin::BorrowedFrom(*obj),
                        Rhs::Prim(p, args) if p.result_borrowed() => match args.first() {
                            Some(Operand::Var(o)) => Origin::BorrowedFrom(*o),
                            _ => Origin::Owned,
                        },
                        Rhs::DropReuse(_) => Origin::Token,
                        _ => Origin::Owned,
                    };
                }
                let multi: &[Var] = match ins {
                    Instr::LetMulti { vars, .. } => vars,
                    Instr::Catch { vars, .. } => vars,
                    _ => &[],
                };
                for v in multi {
                    if f.ty(*v).is_ptr() {
                        origin[v.0 as usize] = Origin::Owned;
                    }
                }
            }
        }
        for (v, s) in f.static_vars(|c| m.fieldless(c)).into_iter().enumerate() {
            if s {
                origin[v] = Origin::Static;
            }
        }
        FnVerifier {
            m,
            f,
            sigs,
            origin,
            token_size: HashMap::new(),
            errs: Vec::new(),
            cur_block: 0,
            points: &[],
            cur_instr: 0,
            held: HashMap::new(),
            unwinding: false,
        }
    }

    /// Records the owned references of `st` at the current instruction if
    /// it is a raising position of kind `when`.
    fn record(&mut self, st: &State, when: crate::unwind::Point) {
        let at = self
            .points
            .get(self.cur_block)
            .and_then(|b| b.get(self.cur_instr));
        if at != Some(&when) {
            return;
        }
        let held: Vec<crate::unwind::Held> = st
            .vars
            .iter()
            .filter(|(_, c)| **c > 0)
            .map(|(v, c)| crate::unwind::Held {
                var: *v,
                count: *c,
                token: self.origin[v.0 as usize] == Origin::Token,
            })
            .collect();
        if !held.is_empty() {
            self.held.insert((self.cur_block, self.cur_instr), held);
        }
    }

    fn name(&self, v: Var) -> &str {
        self.f.var_name(v)
    }

    fn error(&mut self, msg: impl Into<String>) {
        let label = self
            .f
            .blocks
            .get(self.cur_block)
            .map(|b| b.label.as_str())
            .unwrap_or("?");
        let d = Diagnostic::error(format!(
            "in function `{}`, block `{}`: {}",
            self.f.name,
            label,
            msg.into()
        ))
        .with_code("M0100");
        if !self.errs.contains(&d) {
            self.errs.push(d);
        }
    }

    fn run(&mut self) {
        let f = self.f;
        if f.blocks.is_empty() {
            self.error("function has no blocks");
            return;
        }
        if !f.blocks[0].params.is_empty() {
            self.error("the entry block must not have parameters");
        }
        // Each variable defined once.
        let mut defs = vec![0u32; f.var_names.len()];
        for p in &f.params {
            defs[p.var.0 as usize] += 1;
        }
        for b in &f.blocks {
            for p in &b.params {
                defs[p.0 as usize] += 1;
            }
            for ins in &b.instrs {
                for v in ins.defs() {
                    defs[v.0 as usize] += 1;
                }
            }
        }
        for (i, d) in defs.iter().enumerate() {
            if *d > 1 {
                self.error(format!(
                    "variable `{}` is defined {} times",
                    f.var_names[i], d
                ));
            }
        }

        let mut entry = State {
            vars: BTreeMap::new(),
            known: BTreeMap::new(),
        };
        for p in &f.params {
            let owned = matches!(self.origin[p.var.0 as usize], Origin::Owned) as u32;
            entry.vars.insert(p.var, owned);
        }
        let n = f.blocks.len();
        let mut states: Vec<Option<State>> = vec![None; n];
        states[0] = Some(entry);
        let mut work = vec![0usize];
        let mut done = vec![false; n];
        while let Some(bi) = work.pop() {
            if done[bi] {
                continue;
            }
            done[bi] = true;
            self.cur_block = bi;
            let mut st = states[bi].clone().unwrap();
            for (ii, ins) in f.blocks[bi].instrs.iter().enumerate() {
                self.cur_instr = ii;
                self.instr(&mut st, ins);
            }
            let outs = self.term(&mut st, &f.blocks[bi].term);
            for (succ, out) in outs {
                let si = succ.0 as usize;
                if si >= n {
                    continue;
                }
                match &states[si] {
                    None => {
                        states[si] = Some(out);
                        work.push(si);
                    }
                    Some(prev) => {
                        if prev.vars != out.vars {
                            let mut diffs = Vec::new();
                            for v in prev.vars.keys().chain(out.vars.keys()) {
                                let a = prev.vars.get(v).copied().unwrap_or(0);
                                let b = out.vars.get(v).copied().unwrap_or(0);
                                if a != b && !diffs.contains(v) {
                                    diffs.push(*v);
                                }
                            }
                            let names: Vec<String> = diffs
                                .iter()
                                .map(|v| format!("`{}`", self.name(*v)))
                                .collect();
                            if !names.is_empty() {
                                let label = f.blocks[si].label.clone();
                                self.error(format!(
                                    "ownership of {} differs between predecessors of block `{label}`",
                                    names.join(", ")
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    fn alive(&self, st: &State, v: Var, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        match st.vars.get(&v) {
            None => false,
            Some(c) if *c > 0 => true,
            Some(_) => match self.origin[v.0 as usize] {
                Origin::Scalar | Origin::BorrowedParam | Origin::Static => true,
                Origin::BorrowedFrom(p) => self.alive(st, p, depth + 1),
                Origin::Owned | Origin::Token => false,
            },
        }
    }

    fn check_defined(&mut self, st: &State, v: Var) -> bool {
        if !st.vars.contains_key(&v) {
            self.error(format!("use of undefined variable `{}`", self.name(v)));
            return false;
        }
        true
    }

    /// A non-consuming use.
    fn borrow(&mut self, st: &State, v: Var) {
        if !self.check_defined(st, v) {
            return;
        }
        if self.f.ty(v).is_ptr() && !self.alive(st, v, 0) {
            match self.origin[v.0 as usize] {
                Origin::BorrowedFrom(p) => {
                    let pn = self.name(p).to_string();
                    self.error(format!(
                        "use of `{}` after its source `{pn}` was consumed",
                        self.name(v)
                    ))
                }
                _ => self.error(format!("use of consumed variable `{}`", self.name(v))),
            }
        }
    }

    /// A consuming use.
    fn consume(&mut self, st: &mut State, v: Var) {
        if !self.check_defined(st, v) {
            return;
        }
        if !self.f.ty(v).is_ptr() || matches!(self.origin[v.0 as usize], Origin::Static) {
            return;
        }
        let c = st.vars.get_mut(&v).unwrap();
        if *c > 0 {
            *c -= 1;
            return;
        }
        match self.origin[v.0 as usize] {
            Origin::BorrowedParam | Origin::BorrowedFrom(_) => self.error(format!(
                "borrowed variable `{}` is consumed without `dup`",
                self.name(v)
            )),
            _ => self.error(format!(
                "variable `{}` is consumed more than once (double drop or use after free)",
                self.name(v)
            )),
        }
    }

    fn operand(&mut self, st: &mut State, o: &Operand, expect: Option<Ty>, consume: bool) {
        let ty = match o {
            Operand::Var(v) => {
                if consume {
                    self.consume(st, *v);
                } else {
                    self.borrow(st, *v);
                }
                self.f.ty(*v)
            }
            Operand::Const(c) => c.ty(),
        };
        if let Some(e) = expect
            && e != ty
        {
            let what = match o {
                Operand::Var(v) => format!("`{}`", self.name(*v)),
                Operand::Const(c) => crate::print::print_const(c),
            };
            self.error(format!("type mismatch: {what} has type {ty}, expected {e}"));
        }
    }

    fn define(&mut self, st: &mut State, v: Var) {
        let owned = match self.origin[v.0 as usize] {
            Origin::Owned | Origin::Token => 1,
            _ => 0,
        };
        st.vars.insert(v, owned);
    }

    fn args(&mut self, st: &mut State, args: &[Operand], tys: &[Ty], owns: &[Own]) {
        // Borrowed arguments are checked first: they must be alive at the
        // call even when the same variable is also passed owned.
        for (i, a) in args.iter().enumerate() {
            if owns.get(i) == Some(&Own::Borrowed) {
                self.operand(st, a, tys.get(i).copied(), false);
            }
        }
        for (i, a) in args.iter().enumerate() {
            if owns.get(i) != Some(&Own::Borrowed) {
                self.operand(st, a, tys.get(i).copied(), true);
            }
        }
    }

    /// Parameter types, ownership and result types of a known function.
    fn call_sig(&mut self, func: &str) -> Option<(Vec<Ty>, Vec<Own>, Vec<Ty>)> {
        match self.sigs.get(func) {
            Some(g) => Some((
                g.param_tys(),
                g.params.iter().map(|p| p.own).collect(),
                g.results().to_vec(),
            )),
            None => {
                self.error(format!("call to unknown function `{func}`"));
                None
            }
        }
    }

    fn instr(&mut self, st: &mut State, ins: &Instr) {
        let f = self.f;
        self.record(st, crate::unwind::Point::Before);
        match ins {
            Instr::Dup(v) => {
                self.borrow(st, *v);
                if matches!(self.origin[v.0 as usize], Origin::Token) {
                    self.error(format!("`dup` of reuse token `{}`", self.name(*v)));
                }
                if let Some(c) = st.vars.get_mut(v)
                    && !matches!(self.origin[v.0 as usize], Origin::Static)
                {
                    *c += 1;
                }
            }
            Instr::Drop(v) => self.consume(st, *v),
            Instr::Catch { vars, clo, args } => {
                if args.is_empty() || args.len() > MAX_CLOSURE_ARITY {
                    self.error(format!(
                        "`catch apply` takes 1 to {MAX_CLOSURE_ARITY} arguments"
                    ));
                }
                self.operand(st, &Operand::Var(*clo), Some(Ty::Ptr), true);
                let tys = vec![Ty::Ptr; args.len()];
                let owns = vec![Own::Owned; args.len()];
                self.args(st, args, &tys, &owns);
                for (v, t) in vars.iter().zip([Ty::I64, Ty::Ptr]) {
                    if f.ty(*v) != t {
                        self.error(format!(
                            "`{}` is declared {} but `catch apply` binds {t} there",
                            self.name(*v),
                            f.ty(*v)
                        ));
                    }
                }
                for v in vars {
                    self.define(st, *v);
                }
            }
            Instr::LetMulti { vars, func, args } => {
                if let Some((tys, owns, rets)) = self.call_sig(func) {
                    if args.len() != tys.len() {
                        self.error(format!(
                            "`{func}` expects {} arguments, got {}",
                            tys.len(),
                            args.len()
                        ));
                    }
                    self.args(st, args, &tys, &owns);
                    self.record(st, crate::unwind::Point::After);
                    if rets.len() < 2 {
                        self.error(format!(
                            "`{func}` returns a single value; bind it with `let x = call`"
                        ));
                    } else if vars.len() != rets.len() {
                        self.error(format!(
                            "`{func}` returns {} values, but {} variables are bound",
                            rets.len(),
                            vars.len()
                        ));
                    }
                    for (v, t) in vars.iter().zip(&rets) {
                        if f.ty(*v) != *t {
                            self.error(format!(
                                "`{}` is declared {} but the value has type {t}",
                                self.name(*v),
                                f.ty(*v)
                            ));
                        }
                    }
                }
                if vars.len() < 2 {
                    self.error("`let (..) = call` needs at least two variables");
                }
                for v in vars {
                    self.define(st, *v);
                }
            }
            Instr::Set {
                obj,
                ctor,
                field,
                val,
            } => {
                let fty = self.m.ctor(*ctor).fields[*field as usize].ty;
                if st.vars.get(obj).copied().unwrap_or(0) == 0 {
                    self.error(format!(
                        "`set` needs an owned object, but `{}` is not owned here",
                        self.name(*obj)
                    ));
                }
                self.operand(st, val, Some(fty), true);
            }
            Instr::Let { var, rhs } => {
                let result_ty = self.rhs(st, rhs, *var);
                self.record(st, crate::unwind::Point::After);
                match (var, result_ty) {
                    (Some(v), Some(t)) => {
                        if f.ty(*v) != t {
                            self.error(format!(
                                "`{}` is declared {} but the value has type {t}",
                                self.name(*v),
                                f.ty(*v)
                            ));
                        }
                    }
                    (Some(v), None) => self.error(format!(
                        "`{}` is bound to a call returning void",
                        self.name(*v)
                    )),
                    (None, _) => {}
                }
                if let Some(v) = var {
                    self.define(st, *v);
                    if let Rhs::Proj { obj, ctor, .. } = rhs {
                        st.known.insert(*obj, *ctor);
                    }
                    if let Rhs::Alloc { ctor, .. } = rhs {
                        st.known.insert(*v, *ctor);
                    }
                }
            }
        }
    }

    /// Checks a right-hand side and returns its type (None = void).
    fn rhs(&mut self, st: &mut State, rhs: &Rhs, var: Option<Var>) -> Option<Ty> {
        let m = self.m;
        match rhs {
            Rhs::Lit(c) => Some(c.ty()),
            Rhs::Prim(p, args) => {
                let (ptys, ret) = p.signature();
                if args.len() != ptys.len() {
                    self.error(format!(
                        "primitive `{}` expects {} arguments, got {}",
                        p.name(),
                        ptys.len(),
                        args.len()
                    ));
                }
                for (i, a) in args.iter().enumerate() {
                    if !p.consumes().contains(&i) {
                        self.operand(st, a, ptys.get(i).copied(), false);
                    }
                }
                for (i, a) in args.iter().enumerate() {
                    if p.consumes().contains(&i) {
                        self.operand(st, a, ptys.get(i).copied(), true);
                    }
                }
                Some(ret)
            }
            Rhs::Alloc { ctor, args, reuse } => {
                let def = m.ctor(*ctor);
                if args.len() != def.fields.len() {
                    self.error(format!(
                        "constructor `{}` has {} fields, got {} arguments",
                        m.ctor_name(*ctor),
                        def.fields.len(),
                        args.len()
                    ));
                }
                let tys: Vec<Ty> = def.fields.iter().map(|f| f.ty).collect();
                let owns = vec![Own::Owned; args.len()];
                self.args(st, args, &tys, &owns);
                if let Some(tok) = reuse {
                    if !matches!(self.origin[tok.0 as usize], Origin::Token) {
                        self.error(format!(
                            "`reuse {}` needs a token produced by `drop_reuse`",
                            self.name(*tok)
                        ));
                    }
                    if def.fields.is_empty() {
                        self.error("a nullary constructor is static and cannot reuse memory");
                    }
                    let size = self.token_size.get(tok).copied().unwrap_or(0);
                    if size == 0 {
                        self.error(format!(
                            "size of reuse token `{}` is unknown (match the object first)",
                            self.name(*tok)
                        ));
                    } else if size != def.size() {
                        self.error(format!(
                            "reuse token `{}` has size {size} but `{}` needs {}",
                            self.name(*tok),
                            m.ctor_name(*ctor),
                            def.size()
                        ));
                    }
                    self.consume(st, *tok);
                }
                Some(Ty::Ptr)
            }
            Rhs::Proj { obj, ctor, field } => {
                self.operand(st, &Operand::Var(*obj), Some(Ty::Ptr), false);
                if let Some(k) = st.known.get(obj)
                    && k != ctor
                {
                    let (a, b) = (m.ctor_name(*k), m.ctor_name(*ctor));
                    self.error(format!(
                        "`proj` of `{}` as `{b}`, but it is known to be `{a}`",
                        self.name(*obj)
                    ));
                }
                Some(m.ctor(*ctor).fields[*field as usize].ty)
            }
            Rhs::Call { func, args } => {
                let (tys, owns, rets) = self.call_sig(func)?;
                if args.len() != tys.len() {
                    self.error(format!(
                        "`{func}` expects {} arguments, got {}",
                        tys.len(),
                        args.len()
                    ));
                }
                self.args(st, args, &tys, &owns);
                if rets.len() != 1 {
                    self.error(format!(
                        "`{func}` returns {} values; bind them with `let (..) = call`",
                        rets.len()
                    ));
                }
                Some(rets[0])
            }
            Rhs::Apply { clo, args } => {
                if args.is_empty() || args.len() > MAX_CLOSURE_ARITY {
                    self.error(format!("`apply` takes 1 to {MAX_CLOSURE_ARITY} arguments"));
                }
                self.operand(st, &Operand::Var(*clo), Some(Ty::Ptr), true);
                let tys = vec![Ty::Ptr; args.len()];
                let owns = vec![Own::Owned; args.len()];
                self.args(st, args, &tys, &owns);
                Some(Ty::Ptr)
            }
            Rhs::MkClosure { func, args } => {
                if let Some((tys, owns, ret)) = self.call_sig(func) {
                    if args.len() >= tys.len() {
                        self.error(format!(
                            "`mkclosure {func}` captures {} values but `{func}` has only {} parameters",
                            args.len(),
                            tys.len()
                        ));
                    } else {
                        let rest = &tys[args.len()..];
                        if rest.len() > MAX_CLOSURE_ARITY {
                            self.error(format!(
                                "closure arity {} exceeds {MAX_CLOSURE_ARITY}",
                                rest.len()
                            ));
                        }
                        if rest.iter().any(|t| !t.is_ptr())
                            || owns[args.len()..].iter().any(|o| *o != Own::Owned)
                        {
                            self.error(format!("closure parameters of `{func}` must be owned ptr"));
                        }
                    }
                    if ret != [Ty::Ptr] {
                        self.error(format!("closure function `{func}` must return ptr"));
                    }
                    let owns = vec![Own::Owned; args.len()];
                    self.args(st, args, &tys, &owns);
                }
                Some(Ty::Ptr)
            }
            Rhs::MkThunk { func, args } => {
                if let Some((tys, _, ret)) = self.call_sig(func) {
                    if args.len() != tys.len() {
                        self.error(format!(
                            "`mkthunk {func}` must capture all {} parameters",
                            tys.len()
                        ));
                    }
                    if ret != [Ty::Ptr] {
                        self.error(format!("thunk function `{func}` must return ptr"));
                    }
                    let owns = vec![Own::Owned; args.len()];
                    self.args(st, args, &tys, &owns);
                }
                Some(Ty::Ptr)
            }
            Rhs::Force(v) => {
                self.operand(st, &Operand::Var(*v), Some(Ty::Ptr), true);
                Some(Ty::Ptr)
            }
            Rhs::DropReuse(v) => {
                let size = st.known.get(v).map(|c| m.ctor(*c).size()).unwrap_or(0);
                self.operand(st, &Operand::Var(*v), Some(Ty::Ptr), true);
                if let Some(t) = var {
                    self.token_size.insert(t, size);
                }
                Some(Ty::Ptr)
            }
            Rhs::Extern { func, args } => {
                let Some(d) = m.extern_decl(func) else {
                    self.error(format!("undeclared extern `{func}`"));
                    return Some(Ty::I64);
                };
                if args.len() != d.params.len() {
                    self.error(format!(
                        "extern `{func}` expects {} arguments, got {}",
                        d.params.len(),
                        args.len()
                    ));
                }
                let tys: Vec<Ty> = d.params.iter().map(|p| p.0).collect();
                let owns: Vec<Own> = d.params.iter().map(|p| p.1).collect();
                self.args(st, args, &tys, &owns);
                d.ret
            }
            Rhs::Global(g) => {
                if !m.globals.iter().any(|x| &x.name == g) {
                    self.error(format!("unknown global `{g}`"));
                }
                Some(Ty::Ptr)
            }
        }
    }

    fn check_all_consumed(&mut self, st: &State, what: &str) {
        let leaked: Vec<String> = st
            .vars
            .iter()
            .filter(|(_, c)| **c > 0)
            .map(|(v, _)| format!("`{}`", self.name(*v)))
            .collect();
        if !leaked.is_empty() {
            self.error(format!(
                "owned {} {} not consumed before {what} (missing drop / leak)",
                leaked.join(", "),
                if leaked.len() == 1 { "is" } else { "are" }
            ));
        }
    }

    fn term(&mut self, st: &mut State, t: &Terminator) -> Vec<(BlockId, State)> {
        let f = self.f;
        match t {
            Terminator::Ret(o) => {
                if f.is_multi() {
                    self.error(format!(
                        "`ret` of one value in a function returning {} values",
                        f.rets.len()
                    ));
                }
                self.operand(st, o, Some(f.ret), true);
                self.check_all_consumed(st, "`ret`");
                vec![]
            }
            Terminator::RetMulti(os) => {
                if !f.is_multi() {
                    self.error("`ret (..)` in a function returning a single value");
                } else if os.len() != f.rets.len() {
                    self.error(format!(
                        "`ret` of {} values in a function returning {}",
                        os.len(),
                        f.rets.len()
                    ));
                }
                let tys = f.results().to_vec();
                for (i, o) in os.iter().enumerate() {
                    self.operand(st, o, tys.get(i).copied(), true);
                }
                self.check_all_consumed(st, "`ret`");
                vec![]
            }
            Terminator::TailCall { func, args } => {
                if let Some((tys, owns, ret)) = self.call_sig(func) {
                    if args.len() != tys.len() {
                        self.error(format!(
                            "`{func}` expects {} arguments, got {}",
                            tys.len(),
                            args.len()
                        ));
                    }
                    if ret != f.results() {
                        self.error(format!(
                            "tail call to `{func}` returning {} from a function returning {}",
                            show_tys(&ret),
                            show_tys(f.results())
                        ));
                    }
                    if owns.contains(&Own::Borrowed) {
                        // A borrowed argument would outlive the caller's frame
                        // only if the caller keeps ownership; that is fine only
                        // for values the caller itself borrows.
                        for (i, a) in args.iter().enumerate() {
                            if owns[i] == Own::Borrowed
                                && let Operand::Var(v) = a
                                && st.vars.get(v).copied().unwrap_or(0) > 0
                            {
                                self.error(format!(
                                    "`tailcall {func}` passes owned `{}` to a borrowed parameter (it could never be dropped)",
                                    self.name(*v)
                                ));
                            }
                        }
                    }
                    self.args(st, args, &tys, &owns);
                }
                self.check_all_consumed(st, "`tailcall`");
                vec![]
            }
            Terminator::Jump(b, args) => {
                let Some(target) = f.blocks.get(b.0 as usize) else {
                    self.error("jump to unknown block");
                    return vec![];
                };
                if args.len() != target.params.len() {
                    self.error(format!(
                        "jump to `{}` passes {} arguments, block takes {}",
                        target.label,
                        args.len(),
                        target.params.len()
                    ));
                }
                let tys: Vec<Ty> = target.params.iter().map(|p| f.ty(*p)).collect();
                let owns = vec![Own::Owned; args.len()];
                self.args(st, args, &tys, &owns);
                let mut out = st.clone();
                for p in &target.params {
                    self.define(&mut out, *p);
                }
                out.known.clear();
                vec![(*b, out)]
            }
            Terminator::Switch {
                scrut,
                arms,
                default,
            } => {
                self.borrow(st, *scrut);
                let sty = f.ty(*scrut);
                let mut outs = Vec::new();
                let mut targets: Vec<BlockId> = arms.iter().map(|a| a.1).collect();
                targets.extend(default.iter().copied());
                let preds = f.predecessors();
                for (case, b) in arms {
                    match case {
                        Case::Ctor(_) if sty != Ty::Ptr => {
                            self.error("constructor case on a non-pointer value")
                        }
                        Case::Int(_) if sty == Ty::Ptr || sty == Ty::F64 => {
                            self.error("integer case on a non-integer value")
                        }
                        _ => {}
                    }
                    let mut out = st.clone();
                    out.known.clear();
                    let single = targets.iter().filter(|t| *t == b).count() == 1
                        && preds.get(b.0 as usize).is_some_and(|p| p.len() == 1);
                    if let (Case::Ctor(c), true) = (case, single) {
                        out.known.insert(*scrut, *c);
                    }
                    if !f
                        .blocks
                        .get(b.0 as usize)
                        .is_some_and(|x| x.params.is_empty())
                    {
                        self.error("switch targets must not have block parameters");
                    }
                    outs.push((*b, out));
                }
                if let Some(d) = default {
                    let mut out = st.clone();
                    out.known.clear();
                    outs.push((*d, out));
                }
                if arms.is_empty() && default.is_none() {
                    self.error("switch without arms");
                }
                outs
            }
            Terminator::Raise(x) => {
                self.operand(st, &Operand::Var(*x), Some(Ty::Ptr), true);
                self.check_all_consumed(st, "`raise`");
                vec![]
            }
            Terminator::Panic(_) => {
                if self.unwinding {
                    self.check_all_consumed(st, "`panic`");
                }
                vec![]
            }
            Terminator::Unreachable => vec![],
        }
    }
}
