//! Bounds checks hoisted to the loop entry (FEAT-0069).
//!
//! A loop written as a self tail call checks the index of every array
//! access against the array's length in every iteration:
//!
//! ```text
//! fn rev(a: ptr @owned, lo: i64, hi: i64) -> ptr {
//! entry:
//!   let d = prim ge(lo, hi)
//!   switch d { 0 => body, default => done }
//! done:
//!   ret a
//! body:
//!   let x = prim bytes_get_i64(a, lo)
//!   let y = prim bytes_get_i64(a, hi)
//!   ...
//!   let lo1 = prim add(lo, 1)
//!   let hi1 = prim sub(hi, 1)
//!   tailcall rev(a, lo1, hi1)
//! }
//! ```
//!
//! When `lo >= 0` and `hi < len(a)` hold at the start of an iteration, both
//! accesses are in bounds (`lo < hi` on the path to them), and both facts
//! hold again at the start of the next iteration. This pass looks for such
//! an inductive precondition `P` over the integer parameters and the lengths
//! of the array parameters. It adds a copy `f$b` of the function without the
//! checks that `P` proves, whose self tail calls stay in `f$b`, and makes
//! `f` test `P` on entry and continue in `f$b` when it holds. Otherwise `f`
//! runs as before, checking. The removed checks would pass in every
//! iteration that starts with `P`, so no panic moves or disappears.
//!
//! `P` is a conjunction of difference constraints between the entry values
//! (`x >= 0`, `x <= y`, `x < y`, `x <= len(a)`, `x < len(a)`, `len(a) >= c`)
//! chosen from candidates: the non-negative integers first, then, while
//! one does, the candidate (or else the pair of candidates) that proves the
//! most checks. A candidate stays only when every self tail call preserves
//! it (Houdini) and every iteration path stays feasible; at the end, only
//! what the proofs need is kept, preferring the weaker atoms (the fewer and
//! weaker, the more often `P` holds on entry). When the checks hold without
//! any assumption (the loop tests `0 <= i < len(a)` itself), they are
//! removed from `f` in place, without a copy.
//!
//! The body, whose control flow is acyclic, is analysed path by path with a
//! difference bound matrix over the entry values (and 0), refined by the
//! comparisons that `switch`es test and by the checks that passed. A local
//! integer is known as `k * x + c` of an entry value `x` when its arithmetic
//! cannot wrap around (`Int` is two's complement). An array parameter that
//! the function uses directly (not through `force`) is evaluated, so its
//! length can be read on entry; one used through `force` may be a thunk, so
//! the entry test first checks that it is a value (`is_value`, never
//! forcing it): a loop that receives a thunk runs its first iteration
//! checked, forces it there, and enters `f$b` in the next. A self tail call
//! keeps the array when it passes the parameter itself, a `force` or a
//! uniqueness check of it (a copy has the same length).
//!
//! The interpreter treats an unchecked access out of bounds as an internal
//! error, and the code generator keeps the check with the debug runtime, so
//! the run tests check the proofs.

use crate::ir::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

/// Suffix of the function without the hoisted checks.
pub const SUFFIX: &str = "$b";

/// Larger functions are not analysed (compile time).
const MAX_INSTRS: usize = 500;
/// At most this many entry values (0, integer parameters, lengths).
const MAX_NODES: usize = 16;
/// No object is larger than the address space, so no length (in bytes,
/// words or elements) exceeds this.
const LEN_MAX: i128 = 1 << 56;
const I64_MIN: i128 = i64::MIN as i128;
const I64_MAX: i128 = i64::MAX as i128;
/// Largest factor of a known integer (`k * x + c`).
const K_MAX: i128 = 1 << 32;
/// The search for the precondition stops adding candidates after this
/// many analyses of the body (compile time).
const MAX_RUNS: usize = 1000;

/// Adds the functions without hoisted checks to `m`; returns how many
/// functions lost checks (with a copy, or in place when the checks need no
/// precondition).
pub fn hoist_checks(m: &mut Module) -> usize {
    let names: HashSet<String> = m.funcs.iter().map(|f| f.name.clone()).collect();
    let mut new = Vec::new();
    let mut changed = 0;
    for f in &mut m.funcs {
        if f.name.ends_with(SUFFIX) || names.contains(&format!("{}{SUFFIX}", f.name)) {
            continue;
        }
        if let Some(b) = version(f) {
            changed += 1;
            new.extend(b);
        }
    }
    m.funcs.extend(new);
    changed
}

/// Whether `f` would be versioned, and the precondition (for tests).
#[cfg(test)]
pub(crate) fn precondition_of(f: &Function) -> Option<Vec<String>> {
    let a = Analysis::new(f)?;
    let pre = a.precondition()?;
    Some(pre.atoms.iter().map(|t| a.show(t)).collect())
}

/// Versions `f` (see the module documentation) and returns `f$b`; or, when
/// the checks hold without a precondition, removes them from `f` itself
/// (`Some(None)`).
fn version(f: &mut Function) -> Option<Option<Function>> {
    let a = Analysis::new(f)?;
    let pre = a.precondition()?;
    let at: Vec<(usize, usize)> = pre
        .proven
        .iter()
        .map(|&ci| (a.checks[ci].block, a.checks[ci].instr))
        .collect();
    let unchecked = |g: &mut Function| {
        for &(bi, ii) in &at {
            if let Instr::Let {
                rhs: Rhs::Prim(p, _),
                ..
            } = &mut g.blocks[bi].instrs[ii]
            {
                *p = p.unchecked().expect("a bounds-checked access");
            }
        }
    };
    if pre.atoms.is_empty() {
        drop(a);
        unchecked(f);
        return Some(None);
    }
    let mut b = f.clone();
    b.name = format!("{}{SUFFIX}", f.name);
    unchecked(&mut b);
    for blk in &mut b.blocks {
        if let Terminator::TailCall { func, .. } = &mut blk.term
            && *func == f.name
        {
            *func = b.name.clone();
        }
    }
    let test = a.entry_test(&pre.atoms);
    let values = a.lazy_lengths(&pre.atoms);
    drop(a);
    insert_entry_test(f, &values, test, &b.name);
    Some(Some(b))
}

/// A node of the difference bound matrix.
type Node = usize;
const ZERO: Node = 0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Unit {
    /// `bytes_get` / `bytes_set`: bytes.
    Bytes,
    /// The 8-byte accesses of unboxed arrays: `bytes_len / 8`.
    Words,
    /// `array_get` / `array_set`: elements.
    Elems,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NodeKind {
    Zero,
    /// The integer parameter at this position.
    Param(usize),
    /// The length of the array parameter at this position.
    Len(usize, Unit),
}

/// The array an access reads or writes and the unit of its index.
fn access_unit(p: Prim) -> Option<Unit> {
    use Prim::*;
    match p.checked() {
        BytesGet | BytesSet => Some(Unit::Bytes),
        BytesGetI64 | BytesSetI64 | BytesGetF64 | BytesSetF64 | BytesSetI64Shared
        | BytesSetF64Shared => Some(Unit::Words),
        ArrayGet | ArraySet | ArraySetShared => Some(Unit::Elems),
        _ => None,
    }
}

/// Whether `p` reads or writes an element (checked or not; for tests).
#[cfg(test)]
pub(crate) fn is_access(p: Prim) -> bool {
    access_unit(p).is_some()
}

/// Primitives that use their first argument as an evaluated array.
fn uses_array(p: Prim) -> bool {
    access_unit(p).is_some()
        || matches!(
            p,
            Prim::BytesLen | Prim::ArrayLen | Prim::BytesUnique | Prim::ArrayUnique
        )
}

/// `u - v <= c` between entry values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Atom {
    u: Node,
    v: Node,
    c: i128,
}

/// Upper bounds `d[u][v]` of `u - v` (`None`: unbounded), closed under
/// shortest paths.
#[derive(Clone, PartialEq, Debug)]
struct Dbm {
    n: usize,
    d: Vec<Option<i128>>,
}

impl Dbm {
    fn new(n: usize) -> Dbm {
        let mut d = vec![None; n * n];
        for i in 0..n {
            d[i * n + i] = Some(0);
        }
        Dbm { n, d }
    }

    fn get(&self, u: Node, v: Node) -> Option<i128> {
        self.d[u * self.n + v]
    }

    /// Adds `u - v <= c`; false when the constraints become unsatisfiable.
    fn add(&mut self, u: Node, v: Node, c: i128) -> bool {
        if self.get(v, u).is_some_and(|back| back + c < 0) {
            return false;
        }
        if self.get(u, v).is_some_and(|old| old <= c) {
            return true;
        }
        let n = self.n;
        // Every path i -> u -> v -> j may become shorter.
        let to_u: Vec<Option<i128>> = (0..n).map(|i| self.get(i, u)).collect();
        let from_v: Vec<Option<i128>> = (0..n).map(|j| self.get(v, j)).collect();
        for (i, iu) in to_u.iter().enumerate() {
            let Some(iu) = iu else { continue };
            for (j, vj) in from_v.iter().enumerate() {
                let Some(vj) = vj else { continue };
                let w = iu + c + vj;
                let e = &mut self.d[i * n + j];
                if e.is_none_or(|x| w < x) {
                    *e = Some(w);
                }
            }
        }
        true
    }

    /// The least upper bound (both closed, so the result is closed).
    fn join(&mut self, o: &Dbm) {
        for (a, b) in self.d.iter_mut().zip(&o.d) {
            *a = match (*a, *b) {
                (Some(x), Some(y)) => Some(x.max(y)),
                _ => None,
            };
        }
    }

    fn hi(&self, u: Node) -> Option<i128> {
        self.get(u, ZERO)
    }

    fn lo(&self, u: Node) -> Option<i128> {
        self.get(ZERO, u).map(|c| -c)
    }
}

/// `k * x + c` of an entry value `x` (`k >= 1`), exactly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Lin {
    x: Node,
    k: i128,
    c: i128,
}

impl Lin {
    fn konst(c: i128) -> Lin {
        Lin { x: ZERO, k: 1, c }
    }

    fn node(x: Node) -> Lin {
        Lin { x, k: 1, c: 0 }
    }

    fn lo(&self, d: &Dbm) -> Option<i128> {
        Some(self.k * d.lo(self.x)? + self.c)
    }

    fn hi(&self, d: &Dbm) -> Option<i128> {
        Some(self.k * d.hi(self.x)? + self.c)
    }

    /// `self + c` as computed by the machine, when it cannot wrap around.
    fn add(self, d: &Dbm, c: i128) -> Option<Lin> {
        Lin {
            c: self.c.checked_add(c)?,
            ..self
        }
        .exact(d)
    }

    /// `self * m` as computed by the machine, when it cannot wrap around.
    fn mul(self, d: &Dbm, m: i128) -> Option<Lin> {
        if m < 0 {
            return None;
        }
        if m == 0 || self.x == ZERO {
            return Lin::konst(self.c.checked_mul(m)?).exact(d);
        }
        let k = self.k.checked_mul(m)?;
        if k > K_MAX {
            return None;
        }
        Lin {
            x: self.x,
            k,
            c: self.c.checked_mul(m)?,
        }
        .exact(d)
    }

    /// `self` when its value fits in an `i64` in every state of `d`.
    fn exact(self, d: &Dbm) -> Option<Lin> {
        (self.lo(d)? >= I64_MIN && self.hi(d)? <= I64_MAX).then_some(self)
    }
}

fn div_floor(a: i128, b: i128) -> i128 {
    a.div_euclid(b)
}

fn div_ceil(a: i128, b: i128) -> i128 {
    -(-a).div_euclid(b)
}

/// Whether `a - b <= c` holds in every state of `d`.
fn holds(d: &Dbm, a: Lin, b: Lin, c: i128) -> bool {
    if a.k == 1 && b.k == 1 && d.get(a.x, b.x).is_some_and(|w| w + a.c - b.c <= c) {
        return true;
    }
    matches!((a.hi(d), b.lo(d)), (Some(h), Some(l)) if h - l <= c)
}

/// Adds `a - b <= c` to `d` when it is a difference constraint; false when
/// `d` becomes unsatisfiable.
fn assume(d: &mut Dbm, a: Lin, b: Lin, c: i128) -> bool {
    if a.k == 1 && b.k == 1 {
        d.add(a.x, b.x, c - a.c + b.c)
    } else if b.x == ZERO {
        // k * x + ca - cb <= c
        d.add(a.x, ZERO, div_floor(c + b.c - a.c, a.k))
    } else if a.x == ZERO {
        // ca - (k * y + cb) <= c
        d.add(ZERO, b.x, -div_ceil(a.c - b.c - c, b.k))
    } else {
        true
    }
}

/// Adds the outcome `truth` of the comparison `op(a, b)`.
fn assume_cmp(d: &mut Dbm, op: Prim, a: Lin, b: Lin, truth: bool) -> bool {
    use Prim::*;
    let op = match (op, truth) {
        (op, true) => op,
        (Lt, false) => Ge,
        (Le, false) => Gt,
        (Gt, false) => Le,
        (Ge, false) => Lt,
        (Eq, false) => Ne,
        (_, false) => Eq,
    };
    match op {
        Lt => assume(d, a, b, -1),
        Le => assume(d, a, b, 0),
        Gt => assume(d, b, a, -1),
        Ge => assume(d, b, a, 0),
        Eq => assume(d, a, b, 0) && assume(d, b, a, 0),
        // `a <= b` and `a != b` make `a < b` (a loop that stops at `i == n`).
        Ne if holds(d, a, b, 0) => assume(d, a, b, -1),
        Ne if holds(d, b, a, 0) => assume(d, b, a, -1),
        _ => true,
    }
}

/// A bounds-checked access to an array parameter.
struct Check {
    block: usize,
    instr: usize,
    len: Node,
    idx: Operand,
}

/// The state at a self tail call.
#[derive(Clone)]
struct Site {
    d: Dbm,
    /// Per parameter position: the integer argument.
    ints: Vec<Option<Lin>>,
    /// Per parameter position: the array parameter the argument stands for.
    arrays: Vec<Option<usize>>,
}

/// The result of analysing the body under some entry assumptions.
struct Run {
    /// Per check: `None` when unreachable, otherwise whether it is proved.
    proven: Vec<Option<bool>>,
    /// Per check: the bounds of the index when known.
    idx: Vec<Option<(i128, i128)>>,
    /// Per self tail call: `None` when unreachable.
    sites: Vec<Option<Site>>,
    /// Entry values that indices and comparisons depend on.
    used: HashSet<Node>,
}

impl Run {
    fn reaches(&self) -> Vec<bool> {
        self.sites.iter().map(Option::is_some).collect()
    }

    /// How many checks are proved.
    fn proved(&self) -> usize {
        self.proven.iter().filter(|p| **p == Some(true)).count()
    }
}

struct Analysis<'f> {
    f: &'f Function,
    /// Blocks in reverse postorder.
    order: Vec<usize>,
    nodes: Vec<NodeKind>,
    /// Integer parameters and their nodes.
    param_node: Vec<(Var, Node)>,
    len_node: HashMap<(usize, Unit), Node>,
    /// Variables standing for an array parameter (its position).
    alias: HashMap<Var, usize>,
    checks: Vec<Check>,
    check_at: HashMap<(usize, usize), usize>,
    /// Blocks ending in a self tail call.
    tails: Vec<usize>,
    /// Analyses of the body so far.
    runs: Cell<usize>,
    /// Array parameters that may be thunks on entry (used through `force`).
    lazy: HashSet<usize>,
}

/// The precondition and the checks it proves.
struct Pre {
    atoms: Vec<Atom>,
    proven: Vec<usize>,
}

impl<'f> Analysis<'f> {
    fn new(f: &'f Function) -> Option<Analysis<'f>> {
        let tails: Vec<usize> = (0..f.blocks.len())
            .filter(|&b| matches!(&f.blocks[b].term, Terminator::TailCall { func, .. } if *func == f.name))
            .collect();
        let size: usize = f.blocks.iter().map(|b| b.instrs.len()).sum();
        if tails.is_empty() || size > MAX_INSTRS {
            return None;
        }
        let order = reverse_postorder(f)?;
        let pos: HashMap<Var, usize> = f
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| f.ty(p.var).is_ptr())
            .map(|(i, p)| (p.var, i))
            .collect();
        // `force a` of a parameter `a`.
        let mut forced: HashMap<Var, usize> = HashMap::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Instr::Let {
                    var: Some(v),
                    rhs: Rhs::Force(x),
                } = ins
                    && let Some(&k) = pos.get(x)
                {
                    forced.insert(*v, k);
                }
            }
        }
        // Array parameters used directly are evaluated on entry; those used
        // through `force` may still be thunks (`lazy`).
        let mut alias: HashMap<Var, usize> = HashMap::new();
        let mut strict = HashSet::new();
        let mut lazy = HashSet::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Instr::Let {
                    rhs: Rhs::Prim(p, args),
                    ..
                } = ins
                    && uses_array(*p)
                    && let Some(Operand::Var(a)) = args.first()
                {
                    if let Some(&k) = pos.get(a) {
                        strict.insert(k);
                    } else if let Some(&k) = forced.get(a) {
                        lazy.insert(k);
                    } else {
                        continue;
                    }
                }
            }
        }
        for &k in strict.iter().chain(&lazy) {
            alias.insert(f.params[k].var, k);
        }
        if alias.is_empty() {
            return None;
        }
        let lazy: HashSet<usize> = lazy.difference(&strict).copied().collect();
        // Aliases (in an order where definitions come before uses).
        for &bi in &order {
            for ins in &f.blocks[bi].instrs {
                let Instr::Let { var: Some(v), rhs } = ins else {
                    continue;
                };
                let src = match rhs {
                    Rhs::Force(x) => Some(*x),
                    Rhs::Prim(Prim::BytesUnique | Prim::ArrayUnique, args) => args[0].var(),
                    _ => None,
                };
                if let Some(k) = src.and_then(|x| alias.get(&x).copied()) {
                    alias.insert(*v, k);
                }
            }
        }
        let mut nodes = vec![NodeKind::Zero];
        let mut param_node = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            if f.ty(p.var) == Ty::I64 {
                param_node.push((p.var, nodes.len()));
                nodes.push(NodeKind::Param(i));
            }
        }
        let mut len_node = HashMap::new();
        let mut checks = Vec::new();
        let mut check_at = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, ins) in b.instrs.iter().enumerate() {
                let Instr::Let {
                    rhs: Rhs::Prim(p, args),
                    ..
                } = ins
                else {
                    continue;
                };
                let unit = match (access_unit(*p), *p) {
                    (Some(u), _) => u,
                    (None, Prim::BytesLen) => Unit::Bytes,
                    (None, Prim::ArrayLen) => Unit::Elems,
                    _ => continue,
                };
                let Some(&k) = args[0].var().and_then(|a| alias.get(&a)) else {
                    continue;
                };
                let n = *len_node.entry((k, unit)).or_insert_with(|| {
                    nodes.push(NodeKind::Len(k, unit));
                    nodes.len() - 1
                });
                // `bytes_len / 8` of an unboxed array.
                if *p == Prim::BytesLen {
                    len_node.entry((k, Unit::Words)).or_insert_with(|| {
                        nodes.push(NodeKind::Len(k, Unit::Words));
                        nodes.len() - 1
                    });
                }
                if !p.is_unchecked() && access_unit(*p).is_some() {
                    check_at.insert((bi, ii), checks.len());
                    checks.push(Check {
                        block: bi,
                        instr: ii,
                        len: n,
                        idx: args[1].clone(),
                    });
                }
            }
        }
        if checks.is_empty() || nodes.len() > MAX_NODES {
            return None;
        }
        Some(Analysis {
            f,
            order,
            nodes,
            param_node,
            len_node,
            alias,
            checks,
            check_at,
            tails,
            runs: Cell::new(0),
            lazy,
        })
    }

    /// Whether the search has used up its analyses.
    fn spent(&self) -> bool {
        self.runs.get() >= MAX_RUNS
    }

    /// Analyses the body assuming `atoms` on entry; `None` when they
    /// contradict each other.
    fn run(&self, atoms: &[Atom]) -> Option<Run> {
        self.runs.set(self.runs.get() + 1);
        let f = self.f;
        let n = self.nodes.len();
        let mut init = Dbm::new(n);
        for (i, k) in self.nodes.iter().enumerate() {
            match k {
                NodeKind::Zero => {}
                NodeKind::Param(_) => {
                    init.add(i, ZERO, I64_MAX);
                    init.add(ZERO, i, -I64_MIN);
                }
                NodeKind::Len(..) => {
                    init.add(i, ZERO, LEN_MAX);
                    init.add(ZERO, i, 0);
                }
            }
        }
        for a in atoms {
            if !init.add(a.u, a.v, a.c) {
                return None;
            }
        }
        let mut entry: Vec<Option<Dbm>> = vec![None; f.blocks.len()];
        entry[0] = Some(init);
        let mut lin: Vec<Option<Lin>> = vec![None; f.var_tys.len()];
        for &(v, x) in &self.param_node {
            lin[v.0 as usize] = Some(Lin::node(x));
        }
        let mut cmp: HashMap<Var, (Prim, Lin, Lin)> = HashMap::new();
        let mut run = Run {
            proven: vec![None; self.checks.len()],
            idx: vec![None; self.checks.len()],
            sites: vec![None; self.tails.len()],
            used: HashSet::new(),
        };
        let flow = |entry: &mut Vec<Option<Dbm>>, t: BlockId, d: Dbm| {
            let e = &mut entry[t.0 as usize];
            match e {
                Some(old) => old.join(&d),
                None => *e = Some(d),
            }
        };
        for &bi in &self.order {
            let Some(mut d) = entry[bi].take() else {
                continue;
            };
            let b = &f.blocks[bi];
            let mut live = true;
            for (ii, ins) in b.instrs.iter().enumerate() {
                let Instr::Let { var, rhs } = ins else {
                    continue;
                };
                let op = |lin: &[Option<Lin>], o: &Operand| match o {
                    Operand::Var(x) => lin[x.0 as usize],
                    Operand::Const(Const::Int(c, Ty::I64)) => Some(Lin::konst(*c as i128)),
                    Operand::Const(_) => None,
                };
                if let Some(&ci) = self.check_at.get(&(bi, ii)) {
                    let c = &self.checks[ci];
                    let len = Lin::node(c.len);
                    let zero = Lin::konst(0);
                    match op(&lin, &c.idx) {
                        Some(idx) => {
                            run.used.insert(idx.x);
                            run.proven[ci] =
                                Some(holds(&d, zero, idx, 0) && holds(&d, idx, len, -1));
                            run.idx[ci] = idx.lo(&d).zip(idx.hi(&d));
                            // Past the access, the index is in bounds.
                            if !(assume(&mut d, zero, idx, 0) && assume(&mut d, idx, len, -1)) {
                                live = false;
                                break;
                            }
                        }
                        None => run.proven[ci] = Some(false),
                    }
                }
                // A store's result may be unnamed (`let _ = ...`).
                let Some(v) = var else {
                    continue;
                };
                let val = match rhs {
                    Rhs::Lit(Const::Int(c, Ty::I64)) => Some(Lin::konst(*c as i128)),
                    Rhs::Prim(p, args) => {
                        let a = args.first().and_then(|o| op(&lin, o));
                        let b = args.get(1).and_then(|o| op(&lin, o));
                        match (p, a, b) {
                            (Prim::Add, Some(a), Some(b)) if b.x == ZERO => a.add(&d, b.c),
                            (Prim::Add, Some(a), Some(b)) if a.x == ZERO => b.add(&d, a.c),
                            (Prim::Sub, Some(a), Some(b)) if b.x == ZERO => a.add(&d, -b.c),
                            (Prim::Mul, Some(a), Some(b)) if b.x == ZERO => a.mul(&d, b.c),
                            (Prim::Mul, Some(a), Some(b)) if a.x == ZERO => b.mul(&d, a.c),
                            (Prim::LShr, Some(a), Some(b))
                                if a.k == 1 && a.c == 0 && b == Lin::konst(3) =>
                            {
                                match self.nodes[a.x] {
                                    NodeKind::Len(k, Unit::Bytes) => {
                                        self.len_node.get(&(k, Unit::Words)).map(|&w| Lin::node(w))
                                    }
                                    _ => None,
                                }
                            }
                            (Prim::BytesLen | Prim::ArrayLen, _, _) => {
                                let unit = if *p == Prim::BytesLen {
                                    Unit::Bytes
                                } else {
                                    Unit::Elems
                                };
                                args[0]
                                    .var()
                                    .and_then(|x| self.alias.get(&x))
                                    .and_then(|&k| self.len_node.get(&(k, unit)))
                                    .map(|&x| Lin::node(x))
                            }
                            (
                                Prim::Lt | Prim::Le | Prim::Gt | Prim::Ge | Prim::Eq | Prim::Ne,
                                Some(a),
                                Some(b),
                            ) => {
                                run.used.insert(a.x);
                                run.used.insert(b.x);
                                cmp.insert(*v, (*p, a, b));
                                None
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                lin[v.0 as usize] = val;
            }
            if !live {
                continue;
            }
            match &b.term {
                Terminator::Jump(t, _) => flow(&mut entry, *t, d),
                Terminator::Switch {
                    scrut,
                    arms,
                    default,
                } => {
                    let test = cmp.get(scrut).copied();
                    let value = lin[scrut.0 as usize];
                    let ints: Vec<i64> = arms
                        .iter()
                        .filter_map(|(c, _)| match c {
                            Case::Int(k) => Some(*k),
                            Case::Ctor(_) => None,
                        })
                        .collect();
                    for (case, t) in arms {
                        let mut e = d.clone();
                        let ok = match (case, test, value) {
                            (Case::Int(0), Some((op, a, b)), _) => {
                                assume_cmp(&mut e, op, a, b, false)
                            }
                            (Case::Int(1), Some((op, a, b)), _) => {
                                assume_cmp(&mut e, op, a, b, true)
                            }
                            // A comparison is 0 or 1.
                            (Case::Int(_), Some(_), _) => false,
                            (Case::Int(k), None, Some(l)) => {
                                let k = Lin::konst(*k as i128);
                                assume(&mut e, l, k, 0) && assume(&mut e, k, l, 0)
                            }
                            _ => true,
                        };
                        if ok {
                            flow(&mut entry, *t, e);
                        }
                    }
                    if let Some(t) = default {
                        let mut e = d;
                        let ok = match test {
                            Some((op, a, b)) => match (ints.contains(&0), ints.contains(&1)) {
                                (true, false) => assume_cmp(&mut e, op, a, b, true),
                                (false, true) => assume_cmp(&mut e, op, a, b, false),
                                (true, true) => false,
                                (false, false) => true,
                            },
                            None => true,
                        };
                        if ok {
                            flow(&mut entry, *t, e);
                        }
                    }
                }
                Terminator::TailCall { func, args } if *func == f.name => {
                    let si = self.tails.iter().position(|&t| t == bi).unwrap();
                    let op = |o: &Operand| match o {
                        Operand::Var(x) => lin[x.0 as usize],
                        Operand::Const(Const::Int(c, Ty::I64)) => Some(Lin::konst(*c as i128)),
                        Operand::Const(_) => None,
                    };
                    let ints = args.iter().map(op).collect();
                    let arrays = args
                        .iter()
                        .map(|o| o.var().and_then(|x| self.alias.get(&x).copied()))
                        .collect();
                    run.sites[si] = Some(Site { d, ints, arrays });
                }
                _ => {}
            }
        }
        Some(run)
    }

    /// The value of entry value `x` for the next iteration at `s`.
    fn at_site(&self, s: &Site, x: Node) -> Option<Lin> {
        match self.nodes[x] {
            NodeKind::Zero => Some(Lin::konst(0)),
            NodeKind::Param(k) => s.ints[k],
            NodeKind::Len(k, _) => (s.arrays[k] == Some(k)).then_some(Lin::node(x)),
        }
    }

    fn preserves(&self, s: &Site, a: &Atom) -> bool {
        match (self.at_site(s, a.u), self.at_site(s, a.v)) {
            (Some(u), Some(v)) => holds(&s.d, u, v, a.c),
            _ => false,
        }
    }

    fn inductive(&self, r: &Run, atoms: &[Atom]) -> bool {
        r.sites
            .iter()
            .flatten()
            .all(|s| atoms.iter().all(|a| self.preserves(s, a)))
    }

    /// Drops the atoms that some self tail call does not preserve until
    /// the rest are inductive; returns the atoms dropped.
    fn houdini(&self, h: &mut Vec<Atom>) -> Option<(Run, Vec<Atom>)> {
        let r = self.run(h)?;
        self.houdini_from(h, r)
    }

    /// `houdini`, given the analysis under `h`.
    fn houdini_from(&self, h: &mut Vec<Atom>, mut r: Run) -> Option<(Run, Vec<Atom>)> {
        let mut dropped = Vec::new();
        loop {
            let (keep, out): (Vec<Atom>, Vec<Atom>) = h
                .iter()
                .partition(|a| r.sites.iter().flatten().all(|s| self.preserves(s, a)));
            if out.is_empty() {
                return Some((r, dropped));
            }
            dropped.extend(out);
            *h = keep;
            r = self.run(h)?;
        }
    }

    /// Adds the candidates (in order) that keep every iteration path
    /// feasible, then makes the set inductive; repeats while it grows.
    fn grow(&self, h: &mut Vec<Atom>, pool: &[Atom], reach: &[bool]) -> Option<Run> {
        let mut rejected: HashSet<usize> = HashSet::new();
        loop {
            let mut added = false;
            for (i, a) in pool.iter().enumerate() {
                if rejected.contains(&i) || h.contains(a) || self.spent() {
                    continue;
                }
                h.push(*a);
                match self.run(h) {
                    Some(r) if r.reaches() == reach => added = true,
                    _ => {
                        h.pop();
                    }
                }
            }
            let (r, dropped) = self.houdini(h)?;
            for d in dropped {
                if let Some(i) = pool.iter().position(|a| *a == d) {
                    rejected.insert(i);
                }
            }
            if !added || r.reaches() != reach {
                return (r.reaches() == reach).then_some(r);
            }
        }
    }

    /// Adds, while some candidate makes more checks proved, the one that
    /// proves the most (the first of those, so put weaker candidates
    /// first), or else the pair that does (`i <= n` and `n <= len(a)` when
    /// `i != n` guards the loop: neither is enough alone); then all the
    /// candidates that fit together (`grow`) when that proves more still.
    /// Unlike `grow` alone, a candidate that holds in every iteration but
    /// proves nothing cannot shut out a useful one that contradicts it
    /// (`hi < i` against `i <= j` when `j < hi` guards the loop).
    fn select(&self, h: &mut Vec<Atom>, pool: &[Atom], reach: &[bool]) -> Option<Run> {
        let (mut r, _) = self.houdini(h)?;
        loop {
            let base = r.proved();
            // `None` when the atoms do not fit (they contradict `h`, or an
            // iteration path is lost); otherwise their inductive part when
            // it proves more than `base`.
            let fit = |add: &[Atom]| -> Option<Option<(usize, Vec<Atom>, Run)>> {
                let mut more = h.clone();
                more.extend_from_slice(add);
                let r2 = self.run(&more)?;
                if r2.reaches() != reach {
                    return None;
                }
                Some(self.houdini_from(&mut more, r2).and_then(|(r2, _)| {
                    let n = r2.proved();
                    (r2.reaches() == reach && n > base).then_some((n, more, r2))
                }))
            };
            let mut best: Option<(usize, Vec<Atom>, Run)> = None;
            let mut fits = Vec::new();
            for a in pool {
                if h.contains(a) || self.spent() {
                    continue;
                }
                if let Some(c) = fit(&[*a]) {
                    fits.push(*a);
                    if let Some(c) = c
                        && best.as_ref().is_none_or(|b| c.0 > b.0)
                    {
                        best = Some(c);
                    }
                }
            }
            if best.is_none() {
                'pairs: for (k, a) in fits.iter().enumerate() {
                    for b in &fits[k + 1..] {
                        if self.spent() {
                            break 'pairs;
                        }
                        if let Some(Some(c)) = fit(&[*a, *b])
                            && best.as_ref().is_none_or(|b| c.0 > b.0)
                        {
                            best = Some(c);
                        }
                    }
                }
            }
            let Some((_, more, r2)) = best else { break };
            *h = more;
            r = r2;
        }
        let mut all = h.clone();
        if let Some(r2) = self.grow(&mut all, pool, reach)
            && r2.proved() > r.proved()
        {
            *h = all;
            r = r2;
        }
        Some(r)
    }

    fn precondition(&self) -> Option<Pre> {
        let base = self.run(&[])?;
        let reach = base.reaches();
        if !reach.iter().any(|&r| r) {
            return None;
        }
        let ints: Vec<Node> = self.param_node.iter().map(|&(_, x)| x).collect();
        let lens: Vec<Node> = (0..self.nodes.len())
            .filter(|&x| matches!(self.nodes[x], NodeKind::Len(..)))
            .collect();
        let mut h = Vec::new();
        // Non-negative integers first: they make products of them known.
        let pool: Vec<Atom> = ints
            .iter()
            .map(|&x| Atom {
                u: ZERO,
                v: x,
                c: 0,
            })
            .collect();
        let r = self.grow(&mut h, &pool, &reach)?;
        // Relations of the entry values that indices and comparisons use,
        // the weaker of two first.
        let used: Vec<Node> = ints
            .iter()
            .copied()
            .filter(|x| r.used.contains(x))
            .collect();
        let mut pool = Vec::new();
        for &x in &used {
            for &l in &lens {
                pool.push(Atom { u: x, v: l, c: 0 });
                pool.push(Atom { u: x, v: l, c: -1 });
            }
        }
        for &x in &used {
            for &y in &used {
                if x != y {
                    pool.push(Atom { u: x, v: y, c: 0 });
                    pool.push(Atom { u: x, v: y, c: -1 });
                }
            }
        }
        let mut r = self.select(&mut h, &pool, &reach)?;
        // Lengths at least the largest index that nothing else bounds.
        let mut need: HashMap<Node, i128> = HashMap::new();
        for (ci, c) in self.checks.iter().enumerate() {
            if r.proven[ci] == Some(false)
                && let Some((lo, hi)) = r.idx[ci]
                && lo >= 0
                && hi < LEN_MAX
            {
                let e = need.entry(c.len).or_insert(0);
                *e = (*e).max(hi + 1);
            }
        }
        if !need.is_empty() {
            let mut pool: Vec<Atom> = need
                .into_iter()
                .map(|(l, c)| Atom {
                    u: ZERO,
                    v: l,
                    c: -c,
                })
                .collect();
            pool.sort_by_key(|a| a.v);
            r = self.select(&mut h, &pool, &reach)?;
        }
        let goal: Vec<usize> = (0..self.checks.len())
            .filter(|&ci| r.proven[ci] == Some(true))
            .collect();
        // Some proved check must be reachable under `h`.
        if goal.is_empty() || !self.proves_something(&r) {
            return None;
        }
        self.minimize(&mut h, &goal, &reach);
        let r = self.run(&h)?;
        let proven: Vec<usize> = (0..self.checks.len())
            .filter(|&ci| r.proven[ci] != Some(false))
            .collect();
        debug_assert!(self.inductive(&r, &h));
        Some(Pre { atoms: h, proven })
    }

    /// Drops the atoms that the proofs of `goal` do not need, until none
    /// can go. Stronger atoms go first, so that a weaker one stays when it
    /// is enough (`n <= len(a)` rather than `n < len(a)`): the fewer and
    /// weaker the atoms, the more often `P` holds on entry. `h` stays
    /// inductive.
    fn minimize(&self, h: &mut Vec<Atom>, goal: &[usize], reach: &[bool]) {
        let rank = |a: &Atom| {
            let kind = |x: Node| match self.nodes[x] {
                NodeKind::Zero => 0,
                NodeKind::Param(_) => 1,
                NodeKind::Len(..) => 2,
            };
            let strict = a.c < 0;
            match (kind(a.u), kind(a.v)) {
                (1, 1) => [0, 1][!strict as usize],
                (1, 2) => [2, 3][!strict as usize],
                (0, 2) => 4,
                _ => 5,
            }
        };
        loop {
            let mut order = h.clone();
            order.sort_by_key(rank);
            let mut changed = false;
            for a in order {
                if !h.contains(&a) {
                    continue;
                }
                // Without `a`, the atoms that relied on it to be inductive
                // go too.
                let mut fewer: Vec<Atom> = h.iter().copied().filter(|b| *b != a).collect();
                if let Some((r, _)) = self.houdini(&mut fewer)
                    && r.reaches() == reach
                    && goal.iter().all(|&ci| r.proven[ci] != Some(false))
                {
                    *h = fewer;
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn proves_something(&self, r: &Run) -> bool {
        r.proven.contains(&Some(true))
    }

    #[cfg(test)]
    fn show(&self, a: &Atom) -> String {
        let name = |x: Node| match self.nodes[x] {
            NodeKind::Zero => "0".to_string(),
            NodeKind::Param(k) => self.f.var_name(self.f.params[k].var).to_string(),
            NodeKind::Len(k, u) => {
                let a = self.f.var_name(self.f.params[k].var);
                match u {
                    Unit::Bytes => format!("bytes_len({a})"),
                    Unit::Words => format!("words({a})"),
                    Unit::Elems => format!("array_len({a})"),
                }
            }
        };
        match (a.u, a.v, a.c) {
            (ZERO, v, c) => format!("{} >= {}", name(v), -c),
            (u, ZERO, c) => format!("{} <= {c}", name(u)),
            (u, v, 0) => format!("{} <= {}", name(u), name(v)),
            (u, v, -1) => format!("{} < {}", name(u), name(v)),
            (u, v, c) => format!("{} - {} <= {c}", name(u), name(v)),
        }
    }

    /// The lazy array parameters whose lengths `atoms` compare: they must
    /// be values before their lengths are read.
    fn lazy_lengths(&self, atoms: &[Atom]) -> Vec<usize> {
        let mut ks: Vec<usize> = atoms
            .iter()
            .flat_map(|a| [a.u, a.v])
            .filter_map(|x| match self.nodes[x] {
                NodeKind::Len(k, _) if self.lazy.contains(&k) => Some(k),
                _ => None,
            })
            .collect();
        ks.sort();
        ks.dedup();
        ks
    }

    /// The instructions computing `P`, as comparisons of parameters and
    /// lengths.
    fn entry_test(&self, atoms: &[Atom]) -> Vec<Test> {
        atoms
            .iter()
            .map(|a| {
                let val = |x: Node| match self.nodes[x] {
                    NodeKind::Zero => TestVal::Int(0),
                    NodeKind::Param(k) => TestVal::Param(k),
                    NodeKind::Len(k, u) => TestVal::Len(k, u),
                };
                match (a.u, a.v, a.c) {
                    (ZERO, v, c) => Test(Prim::Ge, val(v), TestVal::Int(-c)),
                    (u, ZERO, c) => Test(Prim::Le, val(u), TestVal::Int(c)),
                    (u, v, 0) => Test(Prim::Le, val(u), val(v)),
                    (u, v, -1) => Test(Prim::Lt, val(u), val(v)),
                    _ => unreachable!("candidate atoms have c in {{0, -1}}"),
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TestVal {
    Int(i128),
    Param(usize),
    Len(usize, Unit),
}

/// `op(a, b)` of the entry test.
struct Test(Prim, TestVal, TestVal);

/// Makes `f` test the precondition on entry and tail call `target` when it
/// holds: first that the lazy array parameters `values` are values, then
/// `tests`. The original entry block moves to the end (nothing jumps to it).
fn insert_entry_test(f: &mut Function, values: &[usize], tests: Vec<Test>, target: &str) {
    // `let r = and(all, t)` over the tests `t` computed into `instrs`.
    fn conj(f: &mut Function, instrs: &mut Vec<Instr>, all: Option<Var>, t: Var) -> Var {
        let Some(s) = all else { return t };
        let u = f.fresh_var("pre", Ty::I64);
        instrs.push(Instr::Let {
            var: Some(u),
            rhs: Rhs::Prim(Prim::And, vec![Operand::Var(s), Operand::Var(t)]),
        });
        u
    }
    let mut first = Vec::new();
    let mut evaluated: Option<Var> = None;
    for &k in values {
        let t = f.fresh_var("pre", Ty::I64);
        first.push(Instr::Let {
            var: Some(t),
            rhs: Rhs::Prim(Prim::IsValue, vec![Operand::Var(f.params[k].var)]),
        });
        evaluated = Some(conj(f, &mut first, evaluated, t));
    }
    let mut instrs = Vec::new();
    let mut lens: HashMap<(usize, Unit), Var> = HashMap::new();
    let mut operand = |f: &mut Function, instrs: &mut Vec<Instr>, v: TestVal| match v {
        TestVal::Int(c) => Operand::Const(Const::Int(c as i64, Ty::I64)),
        TestVal::Param(k) => Operand::Var(f.params[k].var),
        TestVal::Len(k, u) => {
            if let Some(&x) = lens.get(&(k, u)) {
                return Operand::Var(x);
            }
            let a = Operand::Var(f.params[k].var);
            let x = f.fresh_var("len", Ty::I64);
            let (p, raw) = match u {
                Unit::Bytes | Unit::Words => (Prim::BytesLen, x),
                Unit::Elems => (Prim::ArrayLen, x),
            };
            instrs.push(Instr::Let {
                var: Some(raw),
                rhs: Rhs::Prim(p, vec![a]),
            });
            let x = if u == Unit::Words {
                let w = f.fresh_var("len", Ty::I64);
                instrs.push(Instr::Let {
                    var: Some(w),
                    rhs: Rhs::Prim(
                        Prim::LShr,
                        vec![Operand::Var(raw), Operand::Const(Const::Int(3, Ty::I64))],
                    ),
                });
                w
            } else {
                raw
            };
            lens.insert((k, u), x);
            Operand::Var(x)
        }
    };
    let mut all: Option<Var> = None;
    for Test(p, a, b) in tests {
        let a = operand(f, &mut instrs, a);
        let b = operand(f, &mut instrs, b);
        let t = f.fresh_var("pre", Ty::I64);
        instrs.push(Instr::Let {
            var: Some(t),
            rhs: Rhs::Prim(p, vec![a, b]),
        });
        all = Some(conj(f, &mut instrs, all, t));
    }
    let all = all.expect("a precondition with at least one atom");
    let labels: HashSet<String> = f.blocks.iter().map(|b| b.label.clone()).collect();
    let fresh = |base: &str| {
        let mut l = base.to_string();
        let mut i = 1;
        while labels.contains(&l) {
            l = format!("{base}{i}");
            i += 1;
        }
        l
    };
    let (checked, unchecked, test) = (fresh("checked"), fresh("unchecked"), fresh("test"));
    // The old entry block goes to `n`, the tail call to `n + 1`, the tests
    // of the lengths (after those of the values) to `n + 2`.
    let n = f.blocks.len() as u32;
    let switch = |scrut: Var, then: u32| Terminator::Switch {
        scrut,
        arms: vec![(Case::Int(0), BlockId(n))],
        default: Some(BlockId(then)),
    };
    let lengths = Block {
        label: String::new(),
        params: Vec::new(),
        instrs,
        term: switch(all, n + 1),
    };
    let (head, rest) = match evaluated {
        None => (lengths, None),
        Some(v) => (
            Block {
                label: String::new(),
                params: Vec::new(),
                instrs: first,
                term: switch(v, n + 2),
            },
            Some(Block {
                label: test,
                ..lengths
            }),
        ),
    };
    let mut old = std::mem::replace(&mut f.blocks[0], head);
    f.blocks[0].label = std::mem::replace(&mut old.label, checked);
    let args = f.params.iter().map(|p| Operand::Var(p.var)).collect();
    f.blocks.push(old);
    f.blocks.push(Block {
        label: unchecked,
        params: Vec::new(),
        instrs: Vec::new(),
        term: Terminator::TailCall {
            func: target.to_string(),
            args,
        },
    });
    f.blocks.extend(rest);
}

/// The blocks in reverse postorder; `None` when the control flow has a
/// cycle or something jumps to the entry block.
fn reverse_postorder(f: &Function) -> Option<Vec<usize>> {
    let n = f.blocks.len();
    if n == 0 {
        return None;
    }
    // 0: unvisited, 1: on the stack, 2: done.
    let mut state = vec![0u8; n];
    let mut post = Vec::with_capacity(n);
    let mut stack: Vec<(usize, Vec<BlockId>, usize)> = vec![(0, f.blocks[0].term.successors(), 0)];
    state[0] = 1;
    while let Some((b, succ, i)) = stack.last_mut() {
        if let Some(&s) = succ.get(*i) {
            *i += 1;
            let s = s.0 as usize;
            match state[s] {
                0 => {
                    state[s] = 1;
                    let next = f.blocks[s].term.successors();
                    stack.push((s, next, 0));
                }
                1 => return None,
                _ => {}
            }
        } else {
            state[*b] = 2;
            post.push(*b);
            stack.pop();
        }
    }
    if f.blocks
        .iter()
        .any(|b| b.term.successors().contains(&BlockId(0)))
    {
        return None;
    }
    post.reverse();
    Some(post)
}
