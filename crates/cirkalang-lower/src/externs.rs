//! Functions of `extern` blocks (spec-v0.0.2.md §8, spec-v0.0.6.md §1): a
//! Core wrapper that unboxes the arguments, calls the function and boxes
//! the result.
//!
//! A value crosses the boundary as one scalar: `Int` / `UInt` / `Char` /
//! `Bool` as an integer, `Float` as a double, `String` and `Handle`
//! (runtime convention only) as the object, and a data type with one
//! constructor and one run-time field as that field. The C types of
//! `Foreign` give the C type; the backend converts at the call
//! (07-backend.md, 08-ffi.md §7.1). The runtime convention also takes
//! functions and actions as their closures (callbacks, spec-v0.0.5.md
//! §2.1). Objects are borrowed by the call; an object result is a new
//! reference.
//!
//! The structs and unions of extern blocks (spec-v0.0.6.md §1.3) cross by
//! value: the wrapper writes the value to C memory with the struct's
//! `$poke` and passes the address, from which the backend passes it by
//! the C convention (08-ffi.md §7.2); a struct result is written to memory
//! that `$peek` reads. Their layouts are computed once for the program
//! (`c_aggs`), and `primCLayout` is replaced by the numbers.

use super::*;

/// The scalar a value is passed as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Base {
    Int,
    UInt,
    Float,
    Char,
    Bool,
    Str,
    /// A runtime object: a `Handle`, or the closure of a callback.
    Obj,
    /// A struct or union, at an address.
    Agg,
}

/// How a value of a type crosses the boundary.
struct Pass {
    /// The constructors to remove, outermost first, with the Core type of
    /// the value they build.
    wraps: Vec<(String, c::Type)>,
    base: Base,
    /// The C type (C convention).
    cty: c::CType,
    /// Of a struct or union.
    agg: Option<AggPass>,
}

#[derive(Clone)]
struct AggPass {
    /// `T.$peek : Int -> IO T` and `T.$poke : Int -> T -> IO ()`.
    peek: String,
    poke: String,
    size: u64,
    /// The Core type of the value.
    ty: c::Type,
}

/// The C type of a type of `Foreign`.
fn foreign_cty(name: &str) -> Option<c::CTy> {
    use c::CTy::*;
    Some(match name.strip_prefix("Foreign.")? {
        "CChar" => I8,
        "CUChar" => U8,
        "CShort" => I16,
        "CUShort" => U16,
        "CInt" => I32,
        "CUInt" => U32,
        "CLong" | "CSSize" => I64,
        "CULong" | "CSize" => U64,
        "CFloat" => F32,
        "CDouble" => F64,
        "CBool" => U8,
        "Ptr" | "OwnedPtr" | "FunPtr" => Ptr,
        _ => return None,
    })
}

/// Whether two declarations of a function call it the same way (a
/// declaration without a C signature uses the runtime convention:
/// `Int#` is `int64_t`, `Double#` is `double`, `Unit#` is `void`).
pub(crate) fn same_call(a: &c::ExternDecl, b: &c::ExternDecl) -> bool {
    fn sig(d: &c::ExternDecl) -> Option<(Vec<c::CType>, c::CType, Option<usize>)> {
        if let Some(s) = &d.c_sig {
            return Some((s.params.clone(), s.ret.clone(), s.variadic));
        }
        let one = |t: &c::Type| {
            if *t == c::Type::int() {
                Some(c::CTy::I64)
            } else if *t == c::Type::double() {
                Some(c::CTy::F64)
            } else if *t == c::Type::unit() {
                Some(c::CTy::Void)
            } else {
                None
            }
        };
        let ps = d
            .params
            .iter()
            .map(|t| one(t).map(c::CType::Scalar))
            .collect::<Option<Vec<_>>>()?;
        Some((ps, c::CType::Scalar(one(&d.ret)?), None))
    }
    a == b || (a.kind == b.kind && a.link == b.link && sig(a).is_some() && sig(a) == sig(b))
}

/// The declarations of `malloc` and `free` (as `Foreign` declares them),
/// for the memory of structs passed by value.
fn malloc_decls() -> [c::ExternDecl; 2] {
    [
        c::ExternDecl {
            name: "malloc".into(),
            params: vec![c::Type::int()],
            ret: c::Type::int(),
            c_sig: Some(c::CSig::scalars(&[c::CTy::U64], c::CTy::Ptr)),
            link: None,
            kind: c::ExternKind::Call,
        },
        c::ExternDecl {
            name: "free".into(),
            params: vec![c::Type::int()],
            ret: c::Type::unit(),
            c_sig: Some(c::CSig::scalars(&[c::CTy::Ptr], c::CTy::Void)),
            link: None,
            kind: c::ExternKind::Call,
        },
    ]
}

/// What an extern function lowers to.
pub(crate) struct ExternOut {
    /// The wrapper (against the representation of `IO`).
    pub body: c::Expr,
    /// The declaration of the C function, then the others it uses.
    pub decls: Vec<c::ExternDecl>,
    /// The handler of a callback.
    pub binds: Vec<c::TopBind>,
}

impl Lower<'_> {
    // ---- C types and layouts ----

    /// The constructor and only run-time field of the data type `d`
    /// applied to `args`, if it has one constructor with one such field.
    fn single_field(&self, d: GName, args: &[(Val, Icit)]) -> Option<(GName, Val)> {
        let Global::Data { cons, nparams, .. } = self.p.get(d).clone() else {
            return None;
        };
        if cons.len() != 1 {
            return None;
        }
        let mut f = self.ev().eval(&Vec::new(), self.p.type_of(cons[0])?);
        for (a, _) in args.iter().take(nparams) {
            match self.whnf(&f) {
                Val::Pi(_, _, _, _, clo) => f = self.ev().inst(&clo, a.clone()),
                _ => return None,
            }
        }
        let mut field = None;
        let mut l = 0;
        while let Val::Pi(_, m, _, a, clo) = self.whnf(&f) {
            if m != Mult::Zero {
                if field.is_some() {
                    return None;
                }
                field = Some(a);
            }
            f = self.ev().inst(&clo, Val::var(l));
            l += 1;
        }
        Some((cons[0], (*field?).clone()))
    }

    /// The C type of a field of a struct or union (checked by the
    /// elaborator).
    fn c_type(&self, v: &Val, depth: u32) -> Option<c::CType> {
        if depth > 16 {
            return None;
        }
        match self.whnf(v) {
            Val::Neutral(Head::Global(g), sp) if sp.is_empty() => {
                Some(c::CType::Scalar(match self.p.prim_of(g)? {
                    Prim::Int => c::CTy::I64,
                    Prim::UInt => c::CTy::U64,
                    Prim::Float => c::CTy::F64,
                    _ => return None,
                }))
            }
            Val::Data(d, args) => {
                if self.p.c_records.contains_key(&d) {
                    return Some(c::CType::Agg(self.gname(d)));
                }
                let name = self.p.name(d);
                if name == "Foreign.CArray" && args.len() == 2 {
                    let Val::Lit(Lit::Int(n)) = self.whnf(&args[0].0) else {
                        return None;
                    };
                    let e = self.c_type(&args[1].0, depth + 1)?;
                    return Some(c::CType::Array(Box::new(e), u32::try_from(n).ok()?));
                }
                if let Some(t) = foreign_cty(name) {
                    return Some(c::CType::Scalar(t));
                }
                let (_, f) = self.single_field(d, &args)?;
                match self.c_type(&f, depth + 1)? {
                    t @ c::CType::Scalar(_) => Some(t),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The field types of a struct, or the member types of a union (from
    /// `U.m : Ptr U -> Ptr M`).
    fn c_record_field_tys(&self, g: GName, r: &CRecord) -> Vec<Val> {
        if r.union {
            return r
                .members
                .iter()
                .filter_map(|m| {
                    let ty = self.ev().eval(&Vec::new(), self.p.type_of(*m)?);
                    let Val::Pi(_, _, _, _, clo) = self.whnf(&ty) else {
                        return None;
                    };
                    match self.whnf(&self.ev().inst(&clo, Val::var(0))) {
                        Val::Data(_, args) => args.first().map(|a| a.0.clone()),
                        _ => None,
                    }
                })
                .collect();
        }
        let Global::Data { cons, .. } = self.p.get(g) else {
            return Vec::new();
        };
        let Some(ct) = cons.first().and_then(|c| self.p.type_of(*c)) else {
            return Vec::new();
        };
        let mut v = self.ev().eval(&Vec::new(), ct);
        let mut out = Vec::new();
        let mut l = 0;
        while let Val::Pi(_, _, _, a, clo) = self.whnf(&v) {
            out.push((*a).clone());
            v = self.ev().inst(&clo, Val::var(l));
            l += 1;
        }
        out
    }

    /// The C aggregates of the program (its structs and unions); their
    /// layouts are kept for `primCLayout` and passing by value.
    pub(crate) fn c_aggs(&self) -> Vec<c::CAgg> {
        let mut recs: Vec<(GName, &CRecord)> =
            self.p.c_records.iter().map(|(g, r)| (*g, r)).collect();
        recs.sort_by_key(|(g, _)| g.0);
        let mut out = Vec::new();
        for (g, r) in &recs {
            let fields: Option<Vec<c::CType>> = self
                .c_record_field_tys(*g, r)
                .iter()
                .map(|t| self.c_type(t, 0))
                .collect();
            let Some(fields) = fields else {
                self.ice(format!("no C type for a field of `{}`", self.gname(*g)));
                continue;
            };
            out.push(c::CAgg {
                name: self.gname(*g),
                union: r.union,
                packed: r.packed,
                fields,
            });
        }
        let types = c::CTypes(&out);
        let mut layouts = self.c_layouts.borrow_mut();
        for (g, _) in &recs {
            match types.layout(&self.gname(*g)) {
                Some(l) => {
                    layouts.insert(*g, l);
                }
                None => self.ice(format!("no layout for `{}`", self.gname(*g))),
            }
        }
        out
    }

    /// `primCLayout {a} p k` (spec-v0.0.6.md §1.9): the size (`k = 0`),
    /// alignment (1) or offset of field `k - 2` of struct or union `a`.
    pub(crate) fn c_layout_app(&self, cx: &Cx, args: &[&Term]) -> Option<i64> {
        let [ty, _, k] = args else {
            return None;
        };
        let Val::Data(d, _) = self.whnf(&self.ev().eval(&cx.env, ty)) else {
            return None;
        };
        let Val::Lit(Lit::Int(k)) = self.whnf(&self.ev().eval(&cx.env, k)) else {
            return None;
        };
        let layouts = self.c_layouts.borrow();
        let l = layouts.get(&d)?;
        let v = match k {
            0 => l.size,
            1 => l.align,
            k => *l.offsets.get(usize::try_from(k - 2).ok()?)?,
        };
        i64::try_from(v).ok()
    }

    fn agg_pass(&self, d: GName) -> Option<Pass> {
        let r = self.p.c_records.get(&d)?;
        let size = self.c_layouts.borrow().get(&d)?.size;
        Some(Pass {
            wraps: Vec::new(),
            base: Base::Agg,
            cty: c::CType::Agg(self.gname(d)),
            agg: Some(AggPass {
                peek: self.gname(r.peek),
                poke: self.gname(r.poke),
                size,
                ty: self.lower_ty(&mut Vec::new(), &Val::Data(d, Vec::new())),
            }),
        })
    }

    // ---- passing values ----

    fn extern_pass(&self, v: &Val, depth: u32) -> Option<Pass> {
        let base = |base, cty| {
            Some(Pass {
                wraps: Vec::new(),
                base,
                cty: c::CType::Scalar(cty),
                agg: None,
            })
        };
        match self.whnf(v) {
            Val::Neutral(Head::Global(g), sp) if sp.is_empty() => match self.p.prim_of(g)? {
                Prim::Int => base(Base::Int, c::CTy::I64),
                Prim::UInt => base(Base::UInt, c::CTy::U64),
                Prim::Float => base(Base::Float, c::CTy::F64),
                Prim::Char => base(Base::Char, c::CTy::U32),
                Prim::String => base(Base::Str, c::CTy::Ptr),
                Prim::Handle => base(Base::Obj, c::CTy::Ptr),
                _ => None,
            },
            Val::Data(d, args) => {
                if Some(d) == self.p.builtins.bool_ {
                    return base(Base::Bool, c::CTy::I32);
                }
                if self.p.c_records.contains_key(&d) {
                    return self.agg_pass(d);
                }
                if depth > 8 {
                    return None;
                }
                let (con, field) = self.single_field(d, &args)?;
                let mut inner = self.extern_pass(&field, depth + 1)?;
                if inner.agg.is_some() {
                    return None;
                }
                // As in the wrapper, written against the representation
                // of `IO` (`FunPtr (A -> IO B)`).
                let outer = io_rep(&self.lower_ty(&mut Vec::new(), &Val::Data(d, args)));
                inner.wraps.insert(0, (self.gname(con), outer));
                if let Some(t) = foreign_cty(self.p.name(d)) {
                    inner.cty = c::CType::Scalar(t);
                }
                Some(inner)
            }
            _ => None,
        }
    }

    /// How a callback parameter (a function or an action) crosses to the
    /// runtime: as its closure.
    fn callback_pass(&self, v: &Val) -> Option<Pass> {
        let closure = match self.whnf(v) {
            Val::Pi(_, _, Icit::Expl, _, _) => true,
            Val::Neutral(Head::Global(h), sp) => {
                sp.len() == 1 && matches!(self.p.prim_of(h), Some(Prim::IO | Prim::IO1))
            }
            _ => false,
        };
        closure.then(|| Pass {
            wraps: Vec::new(),
            base: Base::Obj,
            cty: c::CType::Scalar(c::CTy::Ptr),
            agg: None,
        })
    }

    /// The Core type of the scalar.
    fn base_ty(b: Base) -> c::Type {
        match b {
            Base::Float => c::Type::double(),
            Base::Str => c::Type::bytes(),
            Base::Obj => c::Type::Any,
            _ => c::Type::int(),
        }
    }

    /// The Core type of the value inside the last wrapper.
    fn base_boxed_ty(b: Base) -> c::Type {
        match b {
            Base::Int => c::Type::con("Int"),
            Base::UInt => c::Type::con("UInt"),
            Base::Float => c::Type::con("Float"),
            Base::Char => c::Type::con("Char"),
            Base::Bool | Base::Str | Base::Obj | Base::Agg => c::Type::Any,
        }
    }

    /// Runs the action `io` (of Core type `IO t`) for its value.
    fn run_io(io: c::Expr, t: c::Type) -> c::Expr {
        c::Expr::app(
            c::Expr::Cast(
                Box::new(io),
                c::Type::fun(c::Mult::Many, c::Type::unit(), t),
            ),
            [c::Expr::Lit(c::Lit::Unit)],
        )
    }

    /// `let! x : t = e in body x`.
    fn let_strict(
        &self,
        x: &str,
        t: c::Type,
        e: c::Expr,
        body: impl FnOnce(c::Expr) -> c::Expr,
    ) -> c::Expr {
        let id = self.id(x);
        c::Expr::LetStrict(
            c::Binder::new(id.clone(), c::Mult::Many, t),
            Box::new(e),
            Box::new(body(c::Expr::Var(id))),
        )
    }

    /// `$poke` of a struct: writes `v` at the address `addr` (`Int#`).
    fn poke_agg(&self, a: &AggPass, addr: c::Expr, v: c::Expr) -> c::Expr {
        Self::run_io(
            c::Expr::app(c::Expr::Global(a.poke.clone()), [int_con(addr), v]),
            self.unit_ty(),
        )
    }

    /// Unwraps `e` to its scalar and passes it to `k` (a struct: written
    /// to C memory, whose address is passed, freed after `k`).
    fn pass_in(
        &self,
        e: c::Expr,
        p: &Pass,
        i: usize,
        res: c::Type,
        k: Box<dyn FnOnce(c::Expr) -> c::Expr + '_>,
    ) -> c::Expr {
        if let Some(a) = &p.agg {
            let size = c::Expr::int(i64::try_from(a.size).unwrap_or(i64::MAX));
            return self.let_strict(
                "agg",
                c::Type::int(),
                c::Expr::Extern("malloc".into(), vec![size]),
                |addr| {
                    let poke = self.poke_agg(a, addr.clone(), e);
                    self.let_strict("w", self.unit_ty(), poke, |_| {
                        self.let_strict("r", res, k(addr.clone()), |r| {
                            self.seq_unit(c::Expr::Extern("free".into(), vec![addr]), r)
                        })
                    })
                },
            );
        }
        if let Some((con, ty)) = p.wraps.get(i) {
            let inner = match p.wraps.get(i + 1) {
                Some((_, t)) => t.clone(),
                None if p.base == Base::Bool => self.bool_ty(),
                None if p.base == Base::Str => c::Type::bytes(),
                None => Self::base_boxed_ty(p.base),
            };
            let x = self.id("f");
            let rhs = self.pass_in(c::Expr::Var(x.clone()), p, i + 1, res.clone(), k);
            return c::Expr::Case(
                Box::new(e),
                c::Binder::new(self.id("s"), c::Mult::Many, ty.clone()),
                res,
                vec![c::Alt {
                    con: c::AltCon::Con(con.clone(), vec![c::Binder::new(x, c::Mult::Many, inner)]),
                    rhs,
                }],
            );
        }
        let (con, tyn) = match p.base {
            Base::Int => ("I#", "Int"),
            Base::UInt => ("W#", "UInt"),
            Base::Float => ("F#", "Float"),
            Base::Char => ("C#", "Char"),
            Base::Str | Base::Agg => return k(e),
            Base::Obj => return k(c::Expr::Cast(Box::new(e), c::Type::Any)),
            Base::Bool => {
                let (Some(t), Some(f)) = (self.p.builtins.true_, self.p.builtins.false_) else {
                    return c::Expr::Panic("no Bool".into(), res);
                };
                let b = self.id("b");
                let v = c::Expr::Case(
                    Box::new(e),
                    c::Binder::new(self.id("s"), c::Mult::Many, self.bool_ty()),
                    c::Type::int(),
                    vec![
                        c::Alt {
                            con: c::AltCon::Con(self.gname(f), Vec::new()),
                            rhs: c::Expr::int(0),
                        },
                        c::Alt {
                            con: c::AltCon::Con(self.gname(t), Vec::new()),
                            rhs: c::Expr::int(1),
                        },
                    ],
                );
                return c::Expr::LetStrict(
                    c::Binder::new(b.clone(), c::Mult::Many, c::Type::int()),
                    Box::new(v),
                    Box::new(k(c::Expr::Var(b))),
                );
            }
        };
        self.unbox(e, con, tyn, Self::base_ty(p.base), res, k)
    }

    /// Boxes the scalar `x` into the value (a struct: read from the
    /// address `x`).
    fn pass_out(&self, x: c::Expr, p: &Pass) -> c::Expr {
        let mut e = match p.base {
            Base::Int => c::Expr::Con("I#".into(), Vec::new(), vec![x]),
            Base::UInt => c::Expr::Con("W#".into(), Vec::new(), vec![x]),
            Base::Float => c::Expr::Con("F#".into(), Vec::new(), vec![x]),
            Base::Char => c::Expr::Con("C#".into(), Vec::new(), vec![x]),
            Base::Bool => self.bool_of(x),
            Base::Str | Base::Obj => x,
            Base::Agg => match &p.agg {
                Some(a) => Self::run_io(
                    c::Expr::app(c::Expr::Global(a.peek.clone()), [int_con(x)]),
                    a.ty.clone(),
                ),
                None => c::Expr::Panic("internal error: no struct".into(), c::Type::Any),
            },
        };
        for (con, ty) in p.wraps.iter().rev() {
            let targs = match ty {
                c::Type::Con(_, a) => a.clone(),
                _ => Vec::new(),
            };
            e = c::Expr::Con(con.clone(), targs, vec![e]);
        }
        e
    }

    /// The explicit parameters, the result (`None`: `()`) and whether the
    /// result is an action, of the type of a function crossing the
    /// boundary.
    fn extern_sig(&self, v: &Val, abi: Abi) -> Option<(Vec<Pass>, Option<Pass>, bool)> {
        let mut params = Vec::new();
        let mut v = v.clone();
        let mut l = 0;
        let result = loop {
            match self.whnf(&v) {
                Val::Pi(_, _, Icit::Impl, _, clo) => {
                    v = self.ev().inst(&clo, Val::var(l));
                    l += 1;
                }
                Val::Pi(_, _, Icit::Expl, a, clo) => {
                    let p = match self.extern_pass(&a, 0) {
                        Some(p) => p,
                        None if abi == Abi::Cirka => self.callback_pass(&a)?,
                        None => return None,
                    };
                    params.push(p);
                    v = self.ev().inst(&clo, Val::var(l));
                    l += 1;
                }
                r => break r,
            }
        };
        let (result, io) = match &result {
            Val::Neutral(Head::Global(h), sp)
                if sp.len() == 1 && matches!(self.p.prim_of(*h), Some(Prim::IO | Prim::IO1)) =>
            {
                match &sp[0] {
                    Elim::App(a, _) => (a.clone(), true),
                    _ => return None,
                }
            }
            _ => (result, false),
        };
        let unit = matches!(self.whnf(&result), Val::Data(d, _) if Some(d) == self.p.builtins.unit);
        let ret = if unit {
            None
        } else {
            Some(self.extern_pass(&result, 0)?)
        };
        Some((params, ret, io))
    }

    /// The C signature of these parameters and result.
    fn c_sig(params: &[Pass], ret: Option<&Pass>, variadic: Option<usize>) -> c::CSig {
        c::CSig {
            params: params.iter().map(|p| p.cty.clone()).collect(),
            ret: ret.map_or(c::CType::Scalar(c::CTy::Void), |p| p.cty.clone()),
            variadic,
        }
    }

    /// The Core parameters and result of a call: the function pointer of a
    /// `"dynamic"` call, the address a struct result is written to, then
    /// the arguments (08-ffi.md §7.2).
    fn core_params(params: &[Pass], ret: Option<&Pass>, dynamic: bool) -> (Vec<c::Type>, c::Type) {
        let (fp, rest) = match params.split_first() {
            Some((f, rest)) if dynamic => (Some(f), rest),
            _ => (None, params),
        };
        let mut ps: Vec<c::Type> = Vec::new();
        ps.extend(fp.map(|p| Self::base_ty(p.base)));
        if ret.is_some_and(|r| r.agg.is_some()) {
            ps.push(c::Type::int());
        }
        ps.extend(rest.iter().map(|p| Self::base_ty(p.base)));
        let r = match ret {
            Some(r) if r.agg.is_none() => Self::base_ty(r.base),
            _ => c::Type::unit(),
        };
        (ps, r)
    }

    /// The parameters and result of the function type `F` of a
    /// `"wrapper"` function (`F -> IO (FunPtr F)`) or of an address
    /// (`FunPtr F`).
    fn fun_ptr_sig(&self, f: &Val, abi: Abi) -> Option<(Vec<Pass>, Option<Pass>, bool)> {
        self.extern_sig(f, abi)
    }

    /// The Core lowering of extern function `g`, or `None` (reported by
    /// the elaborator).
    pub(crate) fn extern_fn(
        &self,
        g: GName,
        ty: &Term,
        abi: Abi,
        info: &ExternInfo,
    ) -> Option<ExternOut> {
        let tv = self.ev().eval(&Vec::new(), ty);
        let cty = self.lower_ty(&mut Vec::new(), &tv);
        let rep = io_rep(&cty);
        // A `"wrapper"` function takes the closure it calls.
        let sig_abi = if info.call == ExternCall::Wrapper {
            Abi::Cirka
        } else {
            abi
        };
        let (params, ret, io) = self.extern_sig(&tv, sig_abi)?;
        let c_abi = abi == Abi::C;
        // The declaration's name: the C name, or one of its own when the
        // symbol is given (one symbol may be declared with several types)
        // or the call is not by name.
        let named = info.symbol.is_some() || info.call != ExternCall::Call;
        let name = if named {
            format!("{}$c", self.gname(g))
        } else {
            self.p.short_name(g).to_string()
        };
        let mut decls = Vec::new();
        let mut binds = Vec::new();
        let mut uses_agg =
            params.iter().any(|p| p.agg.is_some()) || ret.as_ref().is_some_and(|r| r.agg.is_some());
        let dynamic = info.call == ExternCall::Dynamic;
        match info.call {
            ExternCall::Call | ExternCall::Dynamic => {
                let (ps, r) = Self::core_params(&params, ret.as_ref(), dynamic);
                let cparams = if dynamic { &params[1..] } else { &params[..] };
                decls.push(c::ExternDecl {
                    name: name.clone(),
                    params: ps,
                    ret: r,
                    c_sig: (c_abi || dynamic)
                        .then(|| Self::c_sig(cparams, ret.as_ref(), info.variadic)),
                    link: if dynamic { None } else { info.symbol.clone() },
                    kind: if dynamic {
                        c::ExternKind::Dynamic
                    } else {
                        c::ExternKind::Call
                    },
                });
            }
            ExternCall::Addr => {
                // `FunPtr F` (a function, with the signature of `F`) or
                // `Ptr T` (data).
                let fsig = match self.whnf(&tv) {
                    Val::Data(d, args) if self.p.name(d) == "Foreign.FunPtr" && args.len() == 1 => {
                        let (fps, fret, _) = self.fun_ptr_sig(&args[0].0, abi)?;
                        Some(Self::c_sig(&fps, fret.as_ref(), None))
                    }
                    _ => None,
                };
                decls.push(c::ExternDecl {
                    name: name.clone(),
                    params: Vec::new(),
                    ret: c::Type::int(),
                    c_sig: fsig,
                    link: info.symbol.clone(),
                    kind: c::ExternKind::Addr,
                });
            }
            ExternCall::Wrapper => {
                let f = self.wrapper_target(&tv)?;
                let (fps, fret, fio) = self.fun_ptr_sig(&f, abi)?;
                uses_agg |= fps.iter().any(|p| p.agg.is_some())
                    || fret.as_ref().is_some_and(|r| r.agg.is_some());
                let handler = format!("{}$handler", self.gname(g));
                binds.push(self.callback_handler(&handler, &f, &fps, fret.as_ref(), fio));
                decls.push(c::ExternDecl {
                    name: name.clone(),
                    params: vec![c::Type::Any],
                    ret: c::Type::int(),
                    c_sig: Some(Self::c_sig(&fps, fret.as_ref(), None)),
                    link: None,
                    kind: c::ExternKind::Callback(handler),
                });
            }
        }
        if uses_agg {
            decls.extend(malloc_decls());
        }
        let n = params.len();
        let params = Rc::new(params);
        let ret = Rc::new(ret);
        let call = info.call;
        let body = self.skeleton(&rep, n + usize::from(io), |args, res| {
            if call == ExternCall::Wrapper {
                // The closure becomes the environment of the pointer.
                let Some(r) = ret.as_ref().as_ref() else {
                    return c::Expr::Panic("internal error: no pointer".into(), res);
                };
                let f = c::Expr::Cast(Box::new(args[0].clone()), c::Type::Any);
                return self.let_strict(
                    "p",
                    c::Type::int(),
                    c::Expr::Extern(name.clone(), vec![f]),
                    |p| self.pass_out(p, r),
                );
            }
            self.extern_args(
                args,
                params.clone(),
                0,
                Vec::new(),
                &name,
                dynamic,
                ret.clone(),
                res,
            )
        });
        Some(ExternOut { body, decls, binds })
    }

    /// `F` of a `"wrapper"` function `F -> IO (FunPtr F)`.
    fn wrapper_target(&self, v: &Val) -> Option<Val> {
        let mut v = v.clone();
        let mut l = 0;
        loop {
            match self.whnf(&v) {
                Val::Pi(_, _, Icit::Impl, _, clo) => {
                    v = self.ev().inst(&clo, Val::var(l));
                    l += 1;
                }
                Val::Pi(_, _, Icit::Expl, a, _) => return Some((*a).clone()),
                _ => return None,
            }
        }
    }

    /// The handler of a `"wrapper"` callback (08-ffi.md §7.2):
    /// `\env [out] x1 .. xn ->` the closure `env` applied to the boxed C
    /// arguments, run, and its result unboxed. A struct argument is read
    /// from its address; a struct result is written to `out`.
    fn callback_handler(
        &self,
        name: &str,
        f: &Val,
        params: &[Pass],
        ret: Option<&Pass>,
        io: bool,
    ) -> c::TopBind {
        let env = self.id("env");
        let f_core = io_rep(&self.lower_ty(&mut Vec::new(), f));
        let out = ret.filter(|r| r.agg.is_some()).map(|_| self.id("out"));
        let xs: Vec<(c::Id, c::Type)> = params
            .iter()
            .map(|p| (self.id("x"), Self::base_ty(p.base)))
            .collect();
        let mut call = c::Expr::Cast(Box::new(c::Expr::Var(env.clone())), f_core.clone());
        let mut fty = f_core;
        for ((x, _), p) in xs.iter().zip(params) {
            let (a, r) = match fty {
                c::Type::Fun(_, a, r) => (*a, *r),
                other => (c::Type::Any, other),
            };
            let arg = self.pass_out(c::Expr::Var(x.clone()), p);
            call = c::Expr::App(Box::new(call), Box::new(c::Expr::Cast(Box::new(arg), a)));
            fty = r;
        }
        // An action is `Unit# -> a` in its representation.
        let res_ty = match (&fty, io) {
            (c::Type::Fun(_, _, r), true) => (**r).clone(),
            _ => fty.clone(),
        };
        if io {
            call = c::Expr::app(call, [c::Expr::Lit(c::Lit::Unit)]);
        }
        let (body, rep) = match ret {
            None => (
                self.let_strict("v", res_ty, call, |_| c::Expr::Lit(c::Lit::Unit)),
                c::Type::unit(),
            ),
            Some(r) => match (&r.agg, &out) {
                (Some(a), Some(o)) => (
                    self.let_strict("v", res_ty, call, |v| {
                        let poke = self.poke_agg(a, c::Expr::Var(o.clone()), v);
                        self.let_strict("w", self.unit_ty(), poke, |_| c::Expr::Lit(c::Lit::Unit))
                    }),
                    c::Type::unit(),
                ),
                _ => {
                    let t = Self::base_ty(r.base);
                    let t2 = t.clone();
                    (
                        self.let_strict("v", res_ty, call, |v| {
                            self.pass_in(v, r, 0, t2, Box::new(|x| x))
                        }),
                        t,
                    )
                }
            },
        };
        let mut e = body;
        let mut t = rep;
        for (x, xt) in xs.iter().rev() {
            e = c::Expr::Lam(
                c::Binder::new(x.clone(), c::Mult::Many, xt.clone()),
                Box::new(e),
            );
            t = c::Type::fun(c::Mult::Many, xt.clone(), t);
        }
        if let Some(o) = out {
            e = c::Expr::Lam(
                c::Binder::new(o, c::Mult::Many, c::Type::int()),
                Box::new(e),
            );
            t = c::Type::fun(c::Mult::Many, c::Type::int(), t);
        }
        c::TopBind {
            name: name.to_string(),
            ty: c::Type::fun(c::Mult::Many, c::Type::Any, t),
            expr: c::Expr::Lam(
                c::Binder::new(env, c::Mult::Many, c::Type::Any),
                Box::new(e),
            ),
            span: Span::DUMMY,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn extern_args(
        &self,
        args: Vec<c::Expr>,
        params: Rc<Vec<Pass>>,
        i: usize,
        mut done: Vec<c::Expr>,
        sym: &str,
        dynamic: bool,
        ret: Rc<Option<Pass>>,
        res: c::Type,
    ) -> c::Expr {
        if i == params.len() {
            return match &*ret {
                None => self.seq_unit(c::Expr::Extern(sym.into(), done), self.unit_val()),
                Some(p) => match &p.agg {
                    // The call writes the result to memory given before the
                    // arguments (after the pointer of a `"dynamic"` call).
                    Some(a) => {
                        let size = c::Expr::int(i64::try_from(a.size).unwrap_or(i64::MAX));
                        self.let_strict(
                            "out",
                            c::Type::int(),
                            c::Expr::Extern("malloc".into(), vec![size]),
                            |out| {
                                done.insert(usize::from(dynamic), out.clone());
                                let read = self.pass_out(out.clone(), p);
                                self.seq_unit(
                                    c::Expr::Extern(sym.into(), done),
                                    self.let_strict("v", res.clone(), read, |v| {
                                        self.seq_unit(c::Expr::Extern("free".into(), vec![out]), v)
                                    }),
                                )
                            },
                        )
                    }
                    None => self.let_strict(
                        "r",
                        Self::base_ty(p.base),
                        c::Expr::Extern(sym.into(), done),
                        |x| self.pass_out(x, p),
                    ),
                },
            };
        }
        let e = args[i].clone();
        let ps = params.clone();
        let sym = sym.to_string();
        let r2 = res.clone();
        self.pass_in(
            e,
            &params[i],
            0,
            res,
            Box::new(move |x| {
                done.push(x);
                self.extern_args(args, ps, i + 1, done, &sym, dynamic, ret, r2)
            }),
        )
    }
}
