//! Loops specialized for unique arrays (FEAT-0067).
//!
//! An in-place update of an array parameter checks its uniqueness first
//! (`bytes_unique` / `array_unique`: the array itself when RC == 1,
//! otherwise a copy). A loop written as a self tail call passes the checked
//! array to the next iteration, which checks it again:
//!
//! ```text
//! fn loop(a: ptr @owned, i: i64) -> ptr {
//!   ...
//!   let c = prim bytes_unique(a)
//!   let _ = prim bytes_set_i64(c, i, x)
//!   tailcall loop(c, i + 1)
//! }
//! ```
//!
//! The argument is known to be unique there, but the function cannot tell
//! its callers apart. This pass adds a specialized function `loop$u` that
//! assumes its array parameters are unique and has those checks removed, and
//! makes the tail calls that pass checked arrays go to it. The loop runs the
//! original until an iteration checks the array, and `loop$u` after that. No
//! check is moved, so no array is copied that the original would not copy.
//!
//! The facts are tracked per program point for each owned pointer
//! parameter `p`: the parameter (or a `force` of it, or a check of either)
//! stays *clean* while it is only read and written in place. Copies of a
//! value (`dup`), passing it to anything else or storing it make it
//! *dirty*. In `loop$u` a clean value is `p` itself, unique because every
//! call of `loop$u` passes clean values, so its `force`s and checks are the
//! identity and are removed. The interpreter traps a store into a shared
//! array, so the run tests check the removal.
//!
//! The first iteration need not run in the original either (FEAT-0070):
//! `loop$u` assumes only that its parameters are values with RC == 1, which
//! the entry of `loop` can test without evaluating or writing anything.
//! [`enter_specialized`] adds that test and moves the old body to a cold
//! function `loop$slow`, which has its own copies of the specialized
//! functions. It runs after the bounds checks are hoisted (`bounds.rs`), so
//! that the test of the loop's precondition stays on the path of the arrays
//! that are not unique, and is made only once, by `loop$u`, on the path of
//! those that are.
//!
//! A parameter that the loop forces and passes on need not be unique
//! (FEAT-0075): `loop$u` can assume it is a value, shared or not, and has no
//! `force` of it. The entry tests only that it is a value. LLVM cannot remove
//! that test across iterations by itself, because the next iteration's
//! argument is the merge of the two results of `force`.
//!
//! The facts cross calls of functions that return their array (FEAT-0076):
//! [`kept_results`] finds, for each function, which result is which
//! parameter used only in place. A loop that passes its array through such
//! calls can be specialized too, and in `loop$u` the calls of functions with
//! a specialized copy go straight to the copy. Such a loop has no check of
//! its own that would bring it from the old body to the copy, so the old
//! body repeats the test of the entry before the next iteration
//! (`loop$again`). Cold code calls the cold functions of other loops: the
//! way through `$slow` stays apart from the way through `$u`, and a loop that
//! starts with a thunk or a shared array runs in `loop$slow$u`, which is
//! optimized like any other function.

use crate::bounds::SUFFIX as BOUNDS_SUFFIX;
use crate::ir::*;
use crate::perceus::{Sigs, instr_uses};
use std::collections::{HashMap, HashSet};

/// Suffix of the specialized function.
pub const SUFFIX: &str = "$u";
/// Suffix of the body of a function entered through its uniqueness test.
pub const SLOW_SUFFIX: &str = "$slow";
/// Suffix of the test the old body repeats before an iteration.
pub const AGAIN_SUFFIX: &str = "$again";

/// What a specialized function assumes about one parameter.
#[derive(Clone, Copy, Debug)]
pub struct Assumed {
    pub position: usize,
    /// It is a value: the copy has no `force` of it, or passes it on to
    /// calls whose facts hold for values only.
    pub forced: bool,
    /// It is unique (RC == 1): the copy has no uniqueness checks of it.
    pub unique: bool,
}

/// A function that got a specialized copy, and what the copy assumes.
#[derive(Clone, Debug)]
pub struct Specialized {
    pub func: String,
    pub target: String,
    pub params: Vec<Assumed>,
    /// Has a function `func$again` that repeats the test of the entry.
    pub again: bool,
}

/// Which parameter each result of a function is (FEAT-0076). Entry `j` is
/// `Some(k)` when result `j` is parameter `k`, used only in place: a caller
/// that passes a unique value gets the same object back, still unique and of
/// the same length. Nothing is known for a thunk: the function may force it
/// and return a shared value.
pub type Kept = HashMap<String, Vec<Option<usize>>>;

/// What the analysis of a function knows about its calls.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    sigs: &'a Sigs,
    kept: &'a Kept,
    /// While computing [`Kept`]: what the calls of the function being
    /// analyzed keep, `(name, result, parameter)`.
    assumed: Option<(&'a str, usize, usize)>,
}

impl Ctx<'_> {
    /// The result of a call of `func` that is the object passed as `h`.
    fn kept_result(&self, func: &str, args: &[Operand], h: Var) -> Option<usize> {
        if mentions(args, h) != 1 {
            return None;
        }
        let k = args.iter().position(|a| a.var() == Some(h))?;
        if let Some((name, j, p)) = self.assumed
            && name == func
        {
            return (p == k).then_some(j);
        }
        self.kept.get(func)?.iter().position(|&x| x == Some(k))
    }
}

/// The results, callee and arguments of a direct call.
fn call_parts(ins: &Instr) -> Option<(&[Var], &str, &[Operand])> {
    match ins {
        Instr::Let {
            var: Some(r),
            rhs: Rhs::Call { func, args },
        } => Some((std::slice::from_ref(r), func, args)),
        Instr::LetMulti { vars, func, args } => Some((vars, func, args)),
        _ => None,
    }
}

/// Which parameter each result of each function keeps. A least fixpoint
/// over calls of other functions; a function's calls of itself are assumed
/// to keep what is being proved (an induction on the depth of the calls).
/// Entries are only added, so the rounds are bounded by their number.
pub fn kept_results(m: &Module) -> Kept {
    let sigs = Sigs::new(m);
    let order = callees_first(m);
    let mut kept = Kept::new();
    loop {
        let mut changed = false;
        for &i in &order {
            let f = &m.funcs[i];
            let new = kept_of(&sigs, &kept, f);
            let cur = kept
                .entry(f.name.clone())
                .or_insert_with(|| vec![None; new.len()]);
            for (c, n) in cur.iter_mut().zip(new) {
                if c.is_none() && n.is_some() {
                    *c = n;
                    changed = true;
                }
            }
        }
        if !changed {
            return kept;
        }
    }
}

fn kept_of(sigs: &Sigs, kept: &Kept, f: &Function) -> Vec<Option<usize>> {
    let results = if f.rets.is_empty() {
        vec![f.ret]
    } else {
        f.rets.clone()
    };
    let mut out = vec![None; results.len()];
    for (k, p) in f.params.iter().enumerate() {
        if p.own != Own::Owned || !f.ty(p.var).is_ptr() {
            continue;
        }
        for j in 0..results.len() {
            if out[j].is_some() || !results[j].is_ptr() {
                continue;
            }
            let c = Ctx {
                sigs,
                kept,
                assumed: Some((&f.name, j, k)),
            };
            let facts = analyze(&c, f, p.var, Mode::Unique);
            if facts
                .exits
                .iter()
                .all(|&(bi, st)| returns_kept(&c, f, st, &f.blocks[bi].term, j, k))
            {
                out[j] = Some(k);
            }
        }
    }
    out
}

/// Whether a block ending in `term`, with parameter `k` in state `st`,
/// returns the parameter as result `j`.
fn returns_kept(c: &Ctx, f: &Function, st: St, term: &Terminator, j: usize, k: usize) -> bool {
    let St::Clean { h, .. } = st else {
        return matches!(term, Terminator::Panic(_) | Terminator::Unreachable);
    };
    match term {
        Terminator::Ret(o) => j == 0 && o.var() == Some(h),
        Terminator::RetMulti(os) => {
            mentions(os, h) == 1 && os.get(j).and_then(Operand::var) == Some(h)
        }
        Terminator::TailCall { func, args } if *func == f.name => {
            mentions(args, h) == 1 && args.get(k).and_then(Operand::var) == Some(h)
        }
        Terminator::TailCall { func, args } => c.kept_result(func, args, h) == Some(j),
        _ => true,
    }
}

/// The functions of `m`, each after the functions it calls (except in
/// call cycles).
fn callees_first(m: &Module) -> Vec<usize> {
    let index: HashMap<&str, usize> = m
        .funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    let succ: Vec<Vec<usize>> = m
        .funcs
        .iter()
        .map(|f| {
            let mut out = Vec::new();
            for b in &f.blocks {
                for ins in &b.instrs {
                    if let Some((_, g, _)) = call_parts(ins)
                        && let Some(&j) = index.get(g)
                    {
                        out.push(j);
                    }
                }
                if let Terminator::TailCall { func, .. } = &b.term
                    && let Some(&j) = index.get(func.as_str())
                {
                    out.push(j);
                }
            }
            out
        })
        .collect();
    let mut seen = vec![false; m.funcs.len()];
    let mut order = Vec::with_capacity(m.funcs.len());
    for root in 0..m.funcs.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some(top) = stack.last_mut() {
            let (v, next) = *top;
            if let Some(&w) = succ[v].get(next) {
                top.1 += 1;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(v);
                stack.pop();
            }
        }
    }
    order
}

/// Adds the specialized functions to `m` and returns what was specialized.
pub fn specialize_loops(m: &mut Module) -> Vec<Specialized> {
    let sigs = Sigs::new(m);
    let kept = kept_results(m);
    let c = Ctx {
        sigs: &sigs,
        kept: &kept,
        assumed: None,
    };
    let names: HashSet<String> = m.funcs.iter().map(|f| f.name.clone()).collect();
    let mut new = Vec::new();
    let mut done = Vec::new();
    let mut by_name: HashMap<String, Specialized> = HashMap::new();
    // Callees first: a caller's copy calls the copies of its callees.
    for i in callees_first(m) {
        let f = &mut m.funcs[i];
        if names.contains(&format!("{}{SUFFIX}", f.name)) {
            continue;
        }
        if let Some((w, s)) = specialize(&c, &by_name, f) {
            new.push(w);
            if s.again {
                new.push(again_function(f, &s));
            }
            by_name.insert(s.func.clone(), s.clone());
            done.push(s);
        }
    }
    m.funcs.extend(new);
    done
}

/// Makes the functions in `done` go to their specialized copies on entry
/// when the parameters are what the copy assumes.
///
/// The way through the entry stays the only way into the specialized loop
/// `f$u` and its copy without bounds checks `f$u$b` (FEAT-0073): the old body
/// `f$slow` gets its own copies of them. LLVM then sees the arguments of the
/// loop at the entry, and a loop called with constants can be unrolled.
pub fn enter_specialized(m: &mut Module, done: &[Specialized]) {
    let index: HashMap<String, usize> = m
        .funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    // The specialized functions of `s`, the first with bounds checks.
    let hot = |s: &Specialized| -> Vec<String> {
        [s.target.clone(), format!("{}{}", s.target, BOUNDS_SUFFIX)]
            .into_iter()
            .filter(|h| index.contains_key(h))
            .collect()
    };
    // What each function of the specialized loops is in the cold code.
    let mut cold: HashMap<String, String> = HashMap::new();
    for s in done {
        let slow = format!("{}{SLOW_SUFFIX}", s.func);
        for h in hot(s) {
            let name = format!("{slow}{}", &h[s.func.len()..]);
            cold.insert(h, name);
        }
        cold.insert(s.func.clone(), slow);
    }
    let mut new = Vec::new();
    for s in done {
        new.push(split_entry(&mut m.funcs[index[&s.func]], s));
        for h in hot(s) {
            let mut copy = m.funcs[index[&h]].clone();
            copy.name = cold[&h].clone();
            new.push(copy);
        }
        // The copy of the old body without bounds checks and the repeated
        // test are reached from that body only.
        if let Some(&b) = index.get(&format!("{}{}", s.func, BOUNDS_SUFFIX)) {
            m.funcs[b].cold = true;
            retarget(&mut m.funcs[b], &cold);
        }
        if let Some(&a) = index.get(&format!("{}{AGAIN_SUFFIX}", s.func)) {
            retarget(&mut m.funcs[a], &cold);
        }
    }
    for f in &mut new {
        retarget(f, &cold);
    }
    m.funcs.extend(new);
}

/// Replaces the callees of the calls of `f` according to `names`.
fn retarget(f: &mut Function, names: &HashMap<String, String>) {
    for b in &mut f.blocks {
        for ins in &mut b.instrs {
            let func = match ins {
                Instr::Let {
                    rhs: Rhs::Call { func, .. },
                    ..
                }
                | Instr::LetMulti { func, .. } => func,
                _ => continue,
            };
            if let Some(to) = names.get(func) {
                *func = to.clone();
            }
        }
        if let Terminator::TailCall { func, .. } = &mut b.term
            && let Some(to) = names.get(func)
        {
            *func = to.clone();
        }
    }
}

/// Moves the body of `f` to a cold function and returns it. `f` itself is
/// left with the test: it tail calls `s.target` when every assumed
/// parameter is unique (and a value if forced), otherwise the old body.
///
/// A separate function, not a block of `f`: the copy of a shared array and
/// the evaluation of a thunk are calls, and registers saved for them would
/// be pushed and popped on every entry of the hot path too.
fn split_entry(f: &mut Function, s: &Specialized) -> Function {
    let mut slow = f.clone();
    slow.name = format!("{}{SLOW_SUFFIX}", f.name);
    slow.cold = true;
    f.blocks = test_blocks(f, s, &slow.name);
    slow
}

/// The test of the entry again, for the iterations of the old body that
/// pass on what the specialized function assumes without having checked it.
fn again_function(f: &Function, s: &Specialized) -> Function {
    let mut g = f.clone();
    g.name = format!("{}{AGAIN_SUFFIX}", f.name);
    g.cold = true;
    g.blocks = test_blocks(&mut g, s, &f.name);
    g
}

/// Blocks that go to `s.target` when the parameters of `f` are what it
/// assumes, and to `other` when not.
fn test_blocks(f: &mut Function, s: &Specialized, other: &str) -> Vec<Block> {
    let mut instrs = Vec::new();
    let mut all: Option<Var> = None;
    let mut test = |f: &mut Function, prim: Prim, k: usize| {
        let arg = Operand::Var(f.params[k].var);
        let t = f.fresh_var("ent", Ty::I64);
        instrs.push(Instr::Let {
            var: Some(t),
            rhs: Rhs::Prim(prim, vec![arg]),
        });
        all = Some(match all {
            None => t,
            Some(a) => {
                let u = f.fresh_var("ent", Ty::I64);
                instrs.push(Instr::Let {
                    var: Some(u),
                    rhs: Rhs::Prim(Prim::And, vec![Operand::Var(a), Operand::Var(t)]),
                });
                u
            }
        });
    };
    for a in &s.params {
        if a.forced {
            test(f, Prim::IsValue, a.position);
        }
        if a.unique {
            test(f, Prim::IsUnique, a.position);
        }
    }
    let scrut = all.expect("a specialized function assumes at least one parameter");
    let args: Vec<Operand> = f.params.iter().map(|p| Operand::Var(p.var)).collect();
    let tail = |label: &str, func: String, args: Vec<Operand>| Block {
        label: label.to_string(),
        params: Vec::new(),
        instrs: Vec::new(),
        term: Terminator::TailCall { func, args },
    };
    vec![
        Block {
            label: "entry".to_string(),
            params: Vec::new(),
            instrs,
            term: Terminator::Switch {
                scrut,
                arms: vec![(Case::Int(0), BlockId(1))],
                default: Some(BlockId(2)),
            },
        },
        tail("not_unique", other.to_string(), args.clone()),
        tail("unique", s.target.clone(), args),
    ]
}

/// The state of one parameter at a program point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum St {
    /// `h` is the parameter (or a `force` / check of it, or the result of
    /// a call that keeps it), used only in place so far; `checked` when a
    /// check has run on this path.
    Clean {
        h: Var,
        checked: bool,
    },
    Dirty,
}

fn is_check(p: Prim) -> bool {
    matches!(p, Prim::BytesUnique | Prim::ArrayUnique)
}

/// Operations that read or write their first argument (an array) in place
/// without keeping a reference to it (with or without the bounds check).
fn in_place(p: Prim) -> bool {
    use Prim::*;
    matches!(
        p.checked(),
        BytesLen
            | BytesGet
            | BytesSet
            | BytesGetI64
            | BytesSetI64
            | BytesGetF64
            | BytesSetF64
            | ArrayLen
            | ArrayGet
            | ArraySet
    )
}

fn mentions(ops: &[Operand], v: Var) -> usize {
    ops.iter().filter(|o| o.var() == Some(v)).count()
}

/// What the analysis of one parameter found.
#[derive(Default)]
struct Facts {
    /// Removable instructions `(block, index)` in `f$u` (a `force` or a
    /// check of a clean value), the variable each one defines and the value
    /// that replaces it.
    removable: Vec<(usize, usize, Var, Var)>,
    /// For each block ending in a self tail call: `Some(checked)` when the
    /// parameter's slot gets the clean value.
    tails: HashMap<usize, Option<bool>>,
    /// Direct calls `(block, index)` that get the clean value, and the
    /// position of that argument.
    calls: HashMap<(usize, usize), usize>,
    /// The clean value goes through a call that keeps it, which says
    /// nothing unless the value is not a thunk.
    through_calls: bool,
    /// The state at the end of each reachable block that leaves the function.
    exits: Vec<(usize, St)>,
}

/// What is tracked about a parameter.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// It is used only in place: the copy assumes it is unique, and
    /// `checked` says that a uniqueness check ran.
    Unique,
    /// It is forced: the copy assumes it is a value, and `checked` says that
    /// a `force` ran. Any other use is fine, a value stays a value.
    Value,
}

/// The state after instruction `ins` of a parameter in state `st`;
/// `removable` gets the value that replaces the result of a removable
/// `force` / check.
fn step(c: &Ctx, mode: Mode, st: St, ins: &Instr, removable: &mut Option<Var>) -> St {
    let St::Clean { h, checked } = st else {
        return St::Dirty;
    };
    if mode == Mode::Value {
        return match ins {
            Instr::Let {
                var: Some(r),
                rhs: Rhs::Force(v),
            } if *v == h => {
                *removable = Some(h);
                St::Clean {
                    h: *r,
                    checked: true,
                }
            }
            _ => st,
        };
    }
    // A call that returns the array keeps what is known about it.
    if let Some((vars, func, args)) = call_parts(ins)
        && let Some(j) = c.kept_result(func, args, h)
        && let Some(&r) = vars.get(j)
    {
        return St::Clean { h: r, checked };
    }
    match ins {
        Instr::Let {
            var: Some(r),
            rhs: Rhs::Force(v),
        } if *v == h => {
            *removable = Some(h);
            St::Clean { h: *r, checked }
        }
        Instr::Let {
            var: Some(r),
            rhs: Rhs::Prim(p, args),
        } if is_check(*p) && args.len() == 1 && args[0].var() == Some(h) => {
            *removable = Some(h);
            St::Clean {
                h: *r,
                checked: true,
            }
        }
        Instr::Let {
            rhs: Rhs::Prim(p, args),
            ..
        } if in_place(*p) && args.first().and_then(Operand::var) == Some(h) => {
            if mentions(&args[1..], h) == 0 {
                st
            } else {
                St::Dirty
            }
        }
        _ => {
            if instr_uses(c.sigs, ins).iter().any(|u| u.0 == h) {
                St::Dirty
            } else {
                st
            }
        }
    }
}

fn join(a: Option<St>, b: St) -> St {
    match a {
        None => b,
        Some(a) if a == b => a,
        Some(_) => St::Dirty,
    }
}

fn analyze(c: &Ctx, f: &Function, param: Var, mode: Mode) -> Facts {
    let n = f.blocks.len();
    let mut entry: Vec<Option<St>> = vec![None; n];
    if n == 0 {
        return Facts::default();
    }
    entry[0] = Some(St::Clean {
        h: param,
        checked: false,
    });
    // Fixpoint over the block entry states (the lattice has height 2).
    let mut work = vec![0usize];
    while let Some(bi) = work.pop() {
        let b = &f.blocks[bi];
        let mut st = entry[bi].unwrap_or(St::Dirty);
        for ins in &b.instrs {
            st = step(c, mode, st, ins, &mut None);
        }
        let out = match (&b.term, st) {
            (Terminator::Jump(_, args), St::Clean { h, .. }) if mentions(args, h) > 0 => St::Dirty,
            (Terminator::Switch { scrut, .. }, St::Clean { h, .. }) if *scrut == h => St::Dirty,
            _ => st,
        };
        for s in b.term.successors() {
            let s = s.0 as usize;
            let j = join(entry[s], out);
            if entry[s] != Some(j) {
                entry[s] = Some(j);
                work.push(s);
            }
        }
    }
    let mut facts = Facts::default();
    for (bi, b) in f.blocks.iter().enumerate() {
        let Some(mut st) = entry[bi] else { continue };
        for (ii, ins) in b.instrs.iter().enumerate() {
            if let St::Clean { h, .. } = st
                && let Some((_, func, args)) = call_parts(ins)
                && mentions(args, h) == 1
                && let Some(k) = args.iter().position(|a| a.var() == Some(h))
            {
                facts.calls.insert((bi, ii), k);
                facts.through_calls |= c.kept_result(func, args, h).is_some();
            }
            let mut rm = None;
            let next = step(c, mode, st, ins, &mut rm);
            if let Some(prev) = rm
                && let Some(&r) = ins.defs().first()
            {
                facts.removable.push((bi, ii, r, prev));
            }
            st = next;
        }
        if matches!(
            b.term,
            Terminator::Ret(_)
                | Terminator::RetMulti(_)
                | Terminator::TailCall { .. }
                | Terminator::Panic(_)
                | Terminator::Unreachable
        ) {
            facts.exits.push((bi, st));
        }
        if let Terminator::TailCall { func, args } = &b.term
            && *func == f.name
        {
            let k = f.params.iter().position(|p| p.var == param);
            let clean = match (st, k) {
                (St::Clean { h, checked }, Some(k))
                    if args.get(k).and_then(Operand::var) == Some(h) && mentions(args, h) == 1 =>
                {
                    Some(checked)
                }
                _ => None,
            };
            facts.tails.insert(bi, clean);
        }
    }
    facts
}

/// A parameter a specialized function makes an assumption about.
struct Chosen {
    var: Var,
    facts: Facts,
    /// Assumed unique (otherwise only a value).
    unique: bool,
    /// Some tail call must pass it in the state `checked`.
    need_checked: bool,
}

/// The specialized function of `f`, retargeting `f`'s tail calls to it.
/// `done` is what was specialized among `f`'s callees.
fn specialize(
    c: &Ctx,
    done: &HashMap<String, Specialized>,
    f: &mut Function,
) -> Option<(Function, Specialized)> {
    let params: Vec<Var> = f
        .params
        .iter()
        .filter(|p| p.own == Own::Owned && f.ty(p.var).is_ptr())
        .map(|p| p.var)
        .collect();
    if params.is_empty() || !has_self_tail_call(f) {
        return None;
    }
    let tail_blocks: Vec<usize> = (0..f.blocks.len())
        .filter(|&bi| matches!(&f.blocks[bi].term, Terminator::TailCall { func, .. } if *func == f.name))
        .collect();
    let passes = |fs: &Facts, need_checked: bool, bi: usize| match fs.tails.get(&bi) {
        Some(Some(c)) => *c || !need_checked,
        _ => false,
    };
    let enters = |set: &[&Facts], need_checked: bool, bi: usize| {
        set.iter().all(|fs| passes(fs, need_checked, bi))
    };
    // Whether some tail call of `f` passes every parameter of `set` in the
    // state it needs.
    let works = |set: &[(&Facts, bool)]| {
        tail_blocks
            .iter()
            .any(|&bi| set.iter().all(|&(fs, need)| passes(fs, need, bi)))
    };
    let facts: Vec<(Var, Facts)> = params
        .iter()
        .map(|&p| (p, analyze(c, f, p, Mode::Unique)))
        .collect();
    let has_check = |fs: &Facts| {
        fs.removable.iter().any(|&(b, ii, _, _)| {
            matches!(&f.blocks[b].instrs[ii], Instr::Let { rhs: Rhs::Prim(p, _), .. } if is_check(*p))
        })
    };
    let calls_copy = |fs: &Facts| {
        fs.calls.keys().any(|&(b, ii)| {
            call_parts(&f.blocks[b].instrs[ii]).is_some_and(|(_, g, _)| done.contains_key(g))
        })
    };
    // Parameters with a check to remove, or passed to a function with a
    // specialized copy.
    let cands: Vec<usize> = (0..facts.len())
        .filter(|&i| has_check(&facts[i].1) || calls_copy(&facts[i].1))
        .collect();
    let need = |i: usize| (&facts[i].1, has_check(&facts[i].1));
    // Assumed unique: all candidates if some tail call of `f` passes all of
    // them as needed, otherwise the first single one that works, if any.
    let all: Vec<(&Facts, bool)> = cands.iter().map(|&i| need(i)).collect();
    let unique: Vec<usize> = if works(&all) {
        cands
    } else {
        cands
            .into_iter()
            .find(|&i| works(&[need(i)]))
            .into_iter()
            .collect()
    };
    let needs: Vec<bool> = (0..facts.len()).map(|i| has_check(&facts[i].1)).collect();
    let mut chosen: Vec<Chosen> = Vec::new();
    let mut facts: Vec<Option<(Var, Facts)>> = facts.into_iter().map(Some).collect();
    for &i in &unique {
        let (var, fs) = facts[i].take().expect("a candidate is chosen once");
        chosen.push(Chosen {
            var,
            facts: fs,
            unique: true,
            need_checked: needs[i],
        });
    }
    // Assumed values: the parameters that are forced and passed on.
    for &p in &params {
        if chosen.iter().any(|ch| ch.var == p) {
            continue;
        }
        let fs = analyze(c, f, p, Mode::Value);
        if fs.removable.is_empty() {
            continue;
        }
        let with: Vec<(&Facts, bool)> = chosen
            .iter()
            .map(|ch| (&ch.facts, ch.need_checked))
            .chain(std::iter::once((&fs, true)))
            .collect();
        if works(&with) {
            chosen.push(Chosen {
                var: p,
                facts: fs,
                unique: false,
                need_checked: true,
            });
        }
    }
    if chosen.is_empty() {
        return None;
    }
    // Calls of functions with a specialized copy go to the copy when this
    // function's assumptions cover what the copy assumes. What the copy
    // assumes to be a value this function assumes too.
    let mut direct: Vec<(usize, usize, String)> = Vec::new();
    let mut used: HashSet<Var> = HashSet::new();
    let mut values: HashSet<Var> = HashSet::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, ins) in b.instrs.iter().enumerate() {
            let Some(s) = call_parts(ins).and_then(|(_, g, _)| done.get(g)) else {
                continue;
            };
            let mut uses = Vec::new();
            let covered = s.params.iter().all(|a| {
                chosen
                    .iter()
                    .find(|ch| ch.facts.calls.get(&(bi, ii)) == Some(&a.position))
                    .is_some_and(|ch| {
                        uses.push((ch.var, a.forced));
                        ch.unique || !a.unique
                    })
            });
            if covered {
                for (v, forced) in uses {
                    used.insert(v);
                    if forced {
                        values.insert(v);
                    }
                }
                direct.push((bi, ii, s.target.clone()));
            }
        }
    }
    // An assumption that removes nothing would only make the entry fail.
    chosen.retain(|ch| !ch.facts.removable.is_empty() || used.contains(&ch.var));
    if chosen.is_empty() {
        return None;
    }
    let position = |v: Var| {
        let k = f.params.iter().position(|p| p.var == v);
        k.expect("a parameter of the function")
    };
    let wname = format!("{}{SUFFIX}", f.name);
    // A call of `f` itself with the clean values goes to the copy, too.
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, ins) in b.instrs.iter().enumerate() {
            if call_parts(ins).is_some_and(|(_, g, _)| g == f.name)
                && chosen
                    .iter()
                    .all(|ch| ch.facts.calls.get(&(bi, ii)) == Some(&position(ch.var)))
            {
                direct.push((bi, ii, wname.clone()));
            }
        }
    }
    let set: Vec<&Facts> = chosen.iter().map(|ch| &ch.facts).collect();
    let assumed = chosen
        .iter()
        .map(|ch| {
            let forced = ch.facts.through_calls
                || values.contains(&ch.var)
                || ch.facts.removable.iter().any(|&(b, ii, _, _)| {
                    matches!(
                        &f.blocks[b].instrs[ii],
                        Instr::Let {
                            rhs: Rhs::Force(_),
                            ..
                        }
                    )
                });
            Assumed {
                position: position(ch.var),
                forced,
                unique: ch.unique,
            }
        })
        .collect();
    let mut w = f.clone();
    w.name = wname.clone();
    for (b, ii, target) in direct {
        match &mut w.blocks[b].instrs[ii] {
            Instr::Let {
                rhs: Rhs::Call { func, .. },
                ..
            }
            | Instr::LetMulti { func, .. } => *func = target,
            _ => unreachable!("a direct call"),
        }
    }
    // Remove the `force`s and checks of clean values in `f$u`.
    let mut replaced: HashMap<Var, Var> = HashMap::new();
    let mut gone: HashSet<(usize, usize)> = HashSet::new();
    for ch in &chosen {
        for &(b, ii, r, prev) in &ch.facts.removable {
            replaced.insert(r, prev);
            gone.insert((b, ii));
        }
    }
    // Follow chains of removed values to the parameter or the call result.
    let subst: HashMap<Var, Var> = replaced
        .keys()
        .map(|&r| {
            let mut x = r;
            while let Some(&y) = replaced.get(&x) {
                x = y;
            }
            (r, x)
        })
        .collect();
    for (bi, b) in w.blocks.iter_mut().enumerate() {
        let instrs = std::mem::take(&mut b.instrs);
        b.instrs = instrs
            .into_iter()
            .enumerate()
            .filter(|(ii, _)| !gone.contains(&(bi, *ii)))
            .map(|(_, ins)| ins)
            .collect();
    }
    rename(&mut w, &subst);
    // The iterations of the old body that pass on all it takes to enter
    // `f$u`, but with some of it not checked, test it at run time.
    let retry = |bi: usize| {
        !enters(&set, true, bi)
            && chosen
                .iter()
                .all(|ch| passes(&ch.facts, ch.need_checked, bi))
    };
    let again = tail_blocks.iter().any(|&bi| retry(bi));
    let again_name = format!("{}{AGAIN_SUFFIX}", f.name);
    for bi in tail_blocks {
        if let Terminator::TailCall { func, .. } = &mut f.blocks[bi].term {
            if enters(&set, true, bi) {
                *func = wname.clone();
            } else if retry(bi) {
                *func = again_name.clone();
            }
        }
        // In `f$u` every clean value is unique, checked or not.
        let to = if enters(&set, false, bi) {
            wname.clone()
        } else {
            f.name.clone()
        };
        if let Terminator::TailCall { func, .. } = &mut w.blocks[bi].term {
            *func = to;
        }
    }
    let info = Specialized {
        func: f.name.clone(),
        target: wname,
        params: assumed,
        again,
    };
    Some((w, info))
}

fn has_self_tail_call(f: &Function) -> bool {
    f.blocks
        .iter()
        .any(|b| matches!(&b.term, Terminator::TailCall { func, .. } if *func == f.name))
}

/// Replaces the uses of the variables in `s` (definitions were removed).
fn rename(f: &mut Function, s: &HashMap<Var, Var>) {
    let r = |v: &mut Var| {
        if let Some(t) = s.get(v) {
            *v = *t;
        }
    };
    let ops = |os: &mut Vec<Operand>| {
        for o in os {
            if let Operand::Var(v) = o
                && let Some(t) = s.get(v)
            {
                *v = *t;
            }
        }
    };
    for b in &mut f.blocks {
        for ins in &mut b.instrs {
            match ins {
                Instr::Let { rhs, .. } => match rhs {
                    Rhs::Lit(_) | Rhs::Global(_) => {}
                    Rhs::Prim(_, a)
                    | Rhs::Call { args: a, .. }
                    | Rhs::MkClosure { args: a, .. }
                    | Rhs::MkThunk { args: a, .. }
                    | Rhs::Extern { args: a, .. } => ops(a),
                    Rhs::Alloc { args, reuse, .. } => {
                        ops(args);
                        if let Some(t) = reuse {
                            r(t);
                        }
                    }
                    Rhs::Proj { obj, .. } => r(obj),
                    Rhs::Apply { clo, args } => {
                        r(clo);
                        ops(args);
                    }
                    Rhs::Force(v) | Rhs::DropReuse(v) => r(v),
                },
                Instr::LetMulti { args, .. } => ops(args),
                Instr::Dup(v) | Instr::Drop(v) => r(v),
                Instr::Set { obj, val, .. } => {
                    r(obj);
                    if let Operand::Var(v) = val
                        && let Some(t) = s.get(v)
                    {
                        *v = *t;
                    }
                }
            }
        }
        match &mut b.term {
            Terminator::Ret(Operand::Var(v)) => r(v),
            Terminator::Ret(_) | Terminator::Panic(_) | Terminator::Unreachable => {}
            Terminator::RetMulti(a)
            | Terminator::Jump(_, a)
            | Terminator::TailCall { args: a, .. } => ops(a),
            Terminator::Switch { scrut, .. } => r(scrut),
        }
    }
}
