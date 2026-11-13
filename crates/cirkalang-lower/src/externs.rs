//! Functions of `extern` blocks (spec-v0.0.2.md §8): a Core wrapper that
//! unboxes the arguments, calls the function and boxes the result.
//!
//! A value crosses the boundary as one scalar: `Int` / `UInt` / `Char` /
//! `Bool` as an integer, `Float` as a double, `String` (runtime convention
//! only) as the string object, and a data type with one constructor and
//! one run-time field as that field. The C types of `Foreign` give the C
//! type; the backend converts at the call (07-backend.md, 08-ffi.md §7.1).

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
}

/// How a value of a type crosses the boundary.
struct Pass {
    /// The constructors to remove, outermost first, with the Core type of
    /// the value they build.
    wraps: Vec<(String, c::Type)>,
    base: Base,
    /// The C type (C convention).
    cty: c::CTy,
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
        "Ptr" | "OwnedPtr" => Ptr,
        _ => return None,
    })
}

/// Whether two declarations of a function call it the same way (a
/// declaration without a C signature uses the runtime convention:
/// `Int#` is `int64_t`, `Double#` is `double`, `Unit#` is `void`).
pub(crate) fn same_call(a: &c::ExternDecl, b: &c::ExternDecl) -> bool {
    fn sig(d: &c::ExternDecl) -> Option<(Vec<c::CTy>, c::CTy)> {
        if let Some(s) = &d.c_sig {
            return Some((s.params.clone(), s.ret));
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
        let ps = d.params.iter().map(one).collect::<Option<Vec<_>>>()?;
        Some((ps, one(&d.ret)?))
    }
    a == b || (sig(a).is_some() && sig(a) == sig(b))
}

impl Lower<'_> {
    fn extern_pass(&self, v: &Val, depth: u32) -> Option<Pass> {
        let base = |base, cty| {
            Some(Pass {
                wraps: Vec::new(),
                base,
                cty,
            })
        };
        match self.whnf(v) {
            Val::Neutral(Head::Global(g), sp) if sp.is_empty() => match self.p.prim_of(g)? {
                Prim::Int => base(Base::Int, c::CTy::I64),
                Prim::UInt => base(Base::UInt, c::CTy::U64),
                Prim::Float => base(Base::Float, c::CTy::F64),
                Prim::Char => base(Base::Char, c::CTy::U32),
                Prim::String => base(Base::Str, c::CTy::Ptr),
                _ => None,
            },
            Val::Data(d, args) => {
                if Some(d) == self.p.builtins.bool_ {
                    return base(Base::Bool, c::CTy::I32);
                }
                let Global::Data { cons, nparams, .. } = self.p.get(d).clone() else {
                    return None;
                };
                if cons.len() != 1 || depth > 8 {
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
                let mut inner = self.extern_pass(&field?.clone(), depth + 1)?;
                let outer = self.lower_ty(&mut Vec::new(), &Val::Data(d, args));
                inner.wraps.insert(0, (self.gname(cons[0]), outer));
                if let Some(t) = foreign_cty(self.p.name(d)) {
                    inner.cty = t;
                }
                Some(inner)
            }
            _ => None,
        }
    }

    /// The Core type of the scalar.
    fn base_ty(b: Base) -> c::Type {
        match b {
            Base::Float => c::Type::double(),
            Base::Str => c::Type::bytes(),
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
            Base::Bool | Base::Str => c::Type::Any,
        }
    }

    /// Unwraps `e` to its scalar and passes it to `k`.
    fn pass_in(
        &self,
        e: c::Expr,
        p: &Pass,
        i: usize,
        res: c::Type,
        k: Box<dyn FnOnce(c::Expr) -> c::Expr + '_>,
    ) -> c::Expr {
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
            Base::Str => return k(e),
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

    /// Boxes the scalar `x` into the value.
    fn pass_out(&self, x: c::Expr, p: &Pass) -> c::Expr {
        let mut e = match p.base {
            Base::Int => c::Expr::Con("I#".into(), Vec::new(), vec![x]),
            Base::UInt => c::Expr::Con("W#".into(), Vec::new(), vec![x]),
            Base::Float => c::Expr::Con("F#".into(), Vec::new(), vec![x]),
            Base::Char => c::Expr::Con("C#".into(), Vec::new(), vec![x]),
            Base::Bool => self.bool_of(x),
            Base::Str => x,
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

    /// The Core wrapper of extern function `g` (written against the
    /// representation of `IO`, which is returned too) and the declaration
    /// of the function, or `None` (reported by the elaborator).
    pub(crate) fn extern_fn(
        &self,
        g: GName,
        ty: &Term,
        abi: Abi,
    ) -> Option<(c::Expr, c::Type, c::ExternDecl)> {
        let tv = self.ev().eval(&Vec::new(), ty);
        let cty = self.lower_ty(&mut Vec::new(), &tv);
        let rep = io_rep(&cty);
        // The parameters, the result and whether it is an action.
        let mut params = Vec::new();
        let mut v = tv;
        let mut l = 0;
        let result = loop {
            match self.whnf(&v) {
                Val::Pi(_, _, Icit::Impl, _, clo) => {
                    v = self.ev().inst(&clo, Val::var(l));
                    l += 1;
                }
                Val::Pi(_, _, Icit::Expl, a, clo) => {
                    params.push(self.extern_pass(&a, 0)?);
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
        let sym = self.p.short_name(g).to_string();
        let c_abi = abi == Abi::C;
        let core_param = |p: &Pass| Self::base_ty(p.base);
        let decl = c::ExternDecl {
            name: sym.clone(),
            params: params.iter().map(core_param).collect(),
            ret: ret.as_ref().map_or(c::Type::unit(), core_param),
            c_sig: c_abi.then(|| c::CSig {
                params: params.iter().map(|p| p.cty).collect(),
                ret: ret.as_ref().map_or(c::CTy::Void, |p| p.cty),
            }),
        };
        let n = params.len();
        let params = Rc::new(params);
        let ret = Rc::new(ret);
        let body = self.skeleton(&rep, n + usize::from(io), |args, res| {
            self.extern_args(args, params.clone(), 0, Vec::new(), &sym, ret.clone(), res)
        });
        Some((body, rep, decl))
    }

    #[allow(clippy::too_many_arguments)]
    fn extern_args(
        &self,
        args: Vec<c::Expr>,
        params: Rc<Vec<Pass>>,
        i: usize,
        mut done: Vec<c::Expr>,
        sym: &str,
        ret: Rc<Option<Pass>>,
        res: c::Type,
    ) -> c::Expr {
        if i == params.len() {
            let call = c::Expr::Extern(sym.into(), done);
            return match &*ret {
                None => self.seq_unit(call, self.unit_val()),
                Some(p) => {
                    let x = self.id("r");
                    c::Expr::LetStrict(
                        c::Binder::new(x.clone(), c::Mult::Many, Self::base_ty(p.base)),
                        Box::new(call),
                        Box::new(self.pass_out(c::Expr::Var(x), p)),
                    )
                }
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
                self.extern_args(args, ps, i + 1, done, &sym, ret, r2)
            }),
        )
    }
}
