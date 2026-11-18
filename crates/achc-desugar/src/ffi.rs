//! `foreign import ccall` (Haskell 2010 §8): a Core `extern` with a C
//! signature and a wrapper that unboxes the arguments and boxes the result.

use crate::Ds;
use achc_typeck::tast;
use achc_typeck::types::*;
use cirka_core as c;
use cirka_core::CTy;
use cirka_diagnostics::Diagnostic;
use std::collections::HashMap;

/// The C type of a marshallable type of the base library (the first one
/// found while unwrapping newtypes decides).
fn c_type_of(name: &str) -> Option<CTy> {
    let (module, short) = name.rsplit_once('.')?;
    if !matches!(
        module,
        "Cirka.Foreign.Types" | "Cirka.Foreign" | "Cirka.Base" | "Cirka.Prim"
    ) {
        return None;
    }
    Some(match short {
        "Int8" | "CChar" | "CSChar" => CTy::I8,
        "Word8" | "CUChar" | "CBool" => CTy::U8,
        "Int16" | "CShort" => CTy::I16,
        "Word16" | "CUShort" => CTy::U16,
        "Int32" | "CInt" | "CWchar" => CTy::I32,
        "Word32" | "CUInt" => CTy::U32,
        "Int" | "Int64" | "CLong" | "CLLong" | "CPtrdiff" | "CSsize" | "CIntPtr" | "CTime" => {
            CTy::I64
        }
        "Word" | "Word64" | "CULong" | "CULLong" | "CSize" | "CUIntPtr" => CTy::U64,
        "Char" => CTy::I32,
        "Double" | "CDouble" => CTy::F64,
        "Float" | "CFloat" => CTy::F32,
        "Ptr" | "FunPtr" => CTy::Ptr,
        _ => return None,
    })
}

/// The boxed primitive a marshallable type is represented by.
#[derive(Clone, Copy, PartialEq)]
enum Base {
    Int,
    Double,
    Char,
}

impl Base {
    fn box_con(self) -> &'static str {
        match self {
            Base::Int => "I#",
            Base::Double => "D#",
            Base::Char => "C#",
        }
    }

    fn core_ty(self) -> c::Type {
        c::Type::con(match self {
            Base::Int => "Int",
            Base::Double => "Double",
            Base::Char => "Char",
        })
    }

    fn unboxed(self) -> c::Type {
        match self {
            Base::Double => c::Type::double(),
            _ => c::Type::int(),
        }
    }
}

/// What a `foreign import ccall` imports (Haskell 2010 §8.5.1).
#[derive(Debug, PartialEq)]
enum Entity {
    /// A C function, called by name.
    Static(String),
    /// The address of a C function or variable (`&name`).
    Addr(String),
    /// A call through a function pointer (`dynamic`).
    Dynamic,
    /// A function pointer made of a Haskell function (`wrapper`).
    Wrapper,
}

/// Parses `"dynamic"`, `"wrapper"` or `"[static] [header.h] [&][name]"`
/// (the name defaults to the Haskell name).
fn entity(entity: &str, hs_name: &str) -> Result<Entity, String> {
    let mut toks: Vec<&str> = entity.split_whitespace().collect();
    match toks.as_slice() {
        ["dynamic"] => return Ok(Entity::Dynamic),
        ["wrapper"] => return Ok(Entity::Wrapper),
        _ => {}
    }
    if toks.first() == Some(&"static") {
        toks.remove(0);
    }
    if toks.first().is_some_and(|t| t.ends_with(".h")) {
        toks.remove(0);
    }
    let own = hs_name.rsplit('.').next().unwrap_or(hs_name);
    let (addr, name) = match toks.as_slice() {
        [] => (false, own),
        ["&"] => (true, own),
        ["&", n] => (true, *n),
        [n] => match n.strip_prefix('&') {
            Some(m) => (true, m),
            None => (false, *n),
        },
        _ => return Err(format!("`{entity}` is not a C function name")),
    };
    let ident = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit());
    if !ident {
        return Err(format!("`{entity}` is not a C function name"));
    }
    Ok(if addr {
        Entity::Addr(name.to_string())
    } else {
        Entity::Static(name.to_string())
    })
}

/// The type constructor of C function pointers.
const FUN_PTR: &str = "Cirka.Foreign.FunPtr";

/// `Maybe`, whose `Nothing` is NULL for the functions of extern blocks.
const MAYBE: &str = "Cirka.Base.Maybe";

/// The borrow monads, newtypes of `IO` that the functions of extern blocks
/// taking borrowed pointers return (08-ffi.md §7.4).
const BORROW_IO: &str = "Unsafe.Extern.BorrowIO";
const BORROW: &str = "Unsafe.Borrow.Borrow";

/// A function type split into its arguments (also of linear arrows), the
/// type of the action when the result is run (`IO t`, and for a function
/// of an extern block also `BorrowIO r t` / `Borrow r t`), and the result.
fn split_fun(t: &Ty, ext: bool) -> (Vec<Ty>, Option<Ty>, Ty) {
    let mut args = Vec::new();
    let mut t = t.clone();
    loop {
        if let Ty::Fun(a, r) = t {
            args.push(*a);
            t = *r;
        } else if let Some((_, a, r)) = as_mul_fun(&t) {
            let (a, r) = (a.clone(), r.clone());
            args.push(a);
            t = r;
        } else {
            break;
        }
    }
    let run = match t.spine() {
        (Ty::Con(h), a) if *h == prim("IO") && a.len() == 1 => Some(a[0].clone()),
        (Ty::Con(h), a) if ext && (h == BORROW_IO || h == BORROW) && a.len() == 2 => {
            Some(a[1].clone())
        }
        _ => None,
    };
    match run {
        Some(r) => (args, Some(t), r),
        None => (args, None, t.clone()),
    }
}

/// The type `FunPtr ft` points to, if `t` is one.
fn fun_ptr_target(t: &Ty) -> Option<Ty> {
    match t.spine() {
        (Ty::Con(h), a) if h == FUN_PTR && a.len() == 1 => Some(a[0].clone()),
        _ => None,
    }
}

fn ffi_error(f: &tast::Foreign, msg: String) -> Diagnostic {
    Diagnostic::error(msg)
        .with_code("E0618")
        .with_label(f.span, "")
}

/// C types of the arguments and the result (None for `()`).
type Marshalled = (Vec<(CTy, Base)>, Option<(CTy, Base)>);

fn scalar_sig((params, ret): &Marshalled, skip: usize) -> c::CSig {
    let cps: Vec<CTy> = params[skip..].iter().map(|p| p.0).collect();
    c::CSig::scalars(&cps, ret.map_or(CTy::Void, |r| r.0))
}

impl Ds {
    /// Unwraps `t` through newtypes to a boxed primitive, picking up the C
    /// type from the first type of the table.
    fn marshal(&self, t: &Ty, datas: &HashMap<&str, &tast::DataDecl>) -> Option<(CTy, Base)> {
        let mut t = t.clone();
        let mut cty = None;
        for _ in 0..32 {
            let (h, args) = t.spine();
            let Ty::Con(name) = h else {
                return None;
            };
            if cty.is_none() {
                cty = c_type_of(name);
            }
            let base = match name.as_str() {
                n if n == prim("Int") => Some(Base::Int),
                n if n == prim("Double") => Some(Base::Double),
                n if n == prim("Char") => Some(Base::Char),
                _ => None,
            };
            if let Some(b) = base {
                return Some((cty?, b));
            }
            let d = datas.get(name.as_str())?;
            if !d.newtype || d.cons.len() != 1 || d.cons[0].fields.len() != 1 {
                return None;
            }
            let sub: Vec<(String, Ty)> = d
                .params
                .iter()
                .map(|p| p.0.clone())
                .zip(args.into_iter().cloned())
                .collect();
            t = d.cons[0].fields[0].subst(&sub);
        }
        None
    }

    /// `Maybe p` of a pointer type `p` (also `Nullable p`): a function of an
    /// extern block passes it as a C pointer that is NULL for `Nothing`
    /// (08-ffi.md §7.4). Returns `p`.
    fn nullable(
        &self,
        f: &tast::Foreign,
        t: &Ty,
        datas: &HashMap<&str, &tast::DataDecl>,
    ) -> Option<Ty> {
        if !f.ext {
            return None;
        }
        match t.spine() {
            (Ty::Con(h), a) if h == MAYBE && a.len() == 1 => self
                .marshal(a[0], datas)
                .filter(|m| m.0 == CTy::Ptr)
                .map(|_| a[0].clone()),
            _ => None,
        }
    }

    /// How `t` crosses the C boundary: a boxed primitive, or a nullable
    /// pointer represented by its address.
    fn marshal_arg(
        &self,
        f: &tast::Foreign,
        t: &Ty,
        datas: &HashMap<&str, &tast::DataDecl>,
    ) -> Option<(CTy, Base)> {
        self.marshal(t, datas)
            .or_else(|| self.nullable(f, t, datas).map(|_| (CTy::Ptr, Base::Int)))
    }

    /// The C types of a function's arguments `args` and result `res`.
    fn marshal_sig(
        &self,
        f: &tast::Foreign,
        args: &[Ty],
        res: &Ty,
        datas: &HashMap<&str, &tast::DataDecl>,
    ) -> Result<Marshalled, Diagnostic> {
        let mut params = Vec::new();
        for a in args {
            match self.marshal_arg(f, a, datas) {
                Some(m) => params.push(m),
                None => {
                    return Err(ffi_error(
                        f,
                        format!("`{}` cannot be passed to a C function", show(a)),
                    )
                    .with_note(if f.ext {
                        "marshallable types are Int, Word, Double, Float, Char, Int8..Word64, \
                         the C types of Foreign.C.Types, Ptr, FunPtr, OwnedPtr, BorrowedPtr, \
                         newtypes of them, and Nullable (Maybe) of a pointer"
                    } else {
                        "marshallable types are Int, Word, Double, Float, Char, Int8..Word64, \
                         the C types of Foreign.C.Types, Ptr and FunPtr, and newtypes of them"
                    }));
                }
            }
        }
        let ret = if *res == Ty::Con(unit_tc()) {
            None
        } else {
            match self.marshal_arg(f, res, datas) {
                Some(m) => Some(m),
                None => {
                    return Err(ffi_error(
                        f,
                        format!("`{}` cannot be returned from a C function", show(res)),
                    ));
                }
            }
        };
        Ok((params, ret))
    }

    /// Desugars one `foreign import ccall` (08-ffi.md §7.1, §7.2).
    pub(crate) fn ccall(
        &mut self,
        f: &tast::Foreign,
        datas: &HashMap<&str, &tast::DataDecl>,
        out: &mut c::Program,
    ) -> Result<(), Diagnostic> {
        let ent = entity(&f.entity, &f.name).map_err(|m| ffi_error(f, m))?;
        let (args, action, res) = split_fun(&f.ty, f.ext);
        let io = action.is_some();
        match ent {
            Entity::Static(name) => {
                self.call_import(f, Some(name), &args, action.as_ref(), &res, datas, out)
            }
            Entity::Dynamic => self.call_import(f, None, &args, action.as_ref(), &res, datas, out),
            Entity::Addr(sym) => self.addr_import(f, &sym, &args, io, &res, datas, out),
            Entity::Wrapper => self.wrapper_import(f, &args, io, &res, datas, out),
        }
    }

    /// A call of the C function `name`, or through the function pointer
    /// passed first (`dynamic`, `name` None): a Core `extern` with a C
    /// signature and a wrapper that unboxes the arguments and boxes the
    /// result (run as the action `action`, when there is one).
    #[allow(clippy::too_many_arguments)]
    fn call_import(
        &mut self,
        f: &tast::Foreign,
        name: Option<String>,
        args: &[Ty],
        action: Option<&Ty>,
        res: &Ty,
        datas: &HashMap<&str, &tast::DataDecl>,
        out: &mut c::Program,
    ) -> Result<(), Diagnostic> {
        let m = self.marshal_sig(f, args, res, datas)?;
        let (params, ret) = (&m.0, m.1);
        let dynamic = name.is_none();
        if dynamic && args.first().and_then(fun_ptr_target).is_none() {
            return Err(ffi_error(
                f,
                "a `dynamic` import takes the function pointer (`FunPtr ft`) first".into(),
            ));
        }
        let name = name.unwrap_or_else(|| format!("{}$dynamic", f.name));
        let decl = c::ExternDecl {
            name: name.clone(),
            params: params.iter().map(|p| p.1.unboxed()).collect(),
            ret: ret.map_or(c::Type::unit(), |r| r.1.unboxed()),
            c_sig: Some(scalar_sig(&m, usize::from(dynamic))),
            link: None,
            kind: if dynamic {
                c::ExternKind::Dynamic
            } else {
                c::ExternKind::Call
            },
        };
        // An extern of the runtime convention (`Int#` = `int64_t`,
        // `Double#` = `double`) is the same function.
        let plain = |e: &c::ExternDecl| {
            let conv = |t: &c::Type| {
                if t.alpha_eq(&c::Type::int()) {
                    Some(CTy::I64)
                } else if t.alpha_eq(&c::Type::double()) {
                    Some(CTy::F64)
                } else if t.alpha_eq(&c::Type::unit()) {
                    Some(CTy::Void)
                } else {
                    None
                }
            };
            let ps: Option<Vec<CTy>> = e.params.iter().map(conv).collect();
            Some(c::CSig::scalars(&ps?, conv(&e.ret)?))
        };
        match out.externs.iter().find(|e| e.name == name) {
            Some(e) if e.c_sig.is_none() && plain(e) == decl.c_sig => {}
            Some(e) if *e != decl => {
                return Err(ffi_error(
                    f,
                    format!("`{name}` is imported with two different C types"),
                ));
            }
            Some(_) => {}
            None => out.externs.push(decl),
        }
        // \a1 .. an -> [\w ->] case a1 of I# x1 -> .. let! r = extern name x1 .. in box r
        let arg_tys: Vec<c::Type> = args.iter().map(|a| self.ty(a)).collect();
        let res_ty = self.ty(res);
        let arg_ids: Vec<c::Id> = (0..args.len()).map(|i| self.id(&format!("a{i}"))).collect();
        let raw: Vec<c::Id> = (0..args.len()).map(|i| self.id(&format!("x{i}"))).collect();
        let call = c::Expr::Extern(name.clone(), raw.iter().map(c::Expr::var).collect());
        let r = self.id("r");
        let result = match ret {
            None => c::Expr::LetStrict(
                c::Binder::new(r, c::Mult::Many, c::Type::unit()),
                Box::new(call),
                Box::new(c::Expr::Con("Unit".into(), Vec::new(), Vec::new())),
            ),
            Some((_, b)) => {
                let boxed = c::Expr::Con(b.box_con().into(), Vec::new(), vec![c::Expr::var(&r)]);
                let value = match self.nullable(f, res, datas) {
                    // case r of 0 -> Nothing; _ -> Just (cast (I# r) p)
                    Some(p) => {
                        let p_ty = self.ty(&p);
                        let s = self.id("s");
                        c::Expr::Case(
                            Box::new(c::Expr::var(&r)),
                            c::Binder::new(s, c::Mult::Many, c::Type::int()),
                            res_ty.clone(),
                            vec![
                                c::Alt {
                                    con: c::AltCon::Int(0),
                                    rhs: c::Expr::Con(
                                        "Cirka.Base.Nothing".into(),
                                        vec![p_ty.clone()],
                                        Vec::new(),
                                    ),
                                },
                                c::Alt {
                                    con: c::AltCon::Default,
                                    rhs: c::Expr::Con(
                                        "Cirka.Base.Just".into(),
                                        vec![p_ty.clone()],
                                        vec![c::Expr::Cast(Box::new(boxed), p_ty)],
                                    ),
                                },
                            ],
                        )
                    }
                    None => c::Expr::Cast(Box::new(boxed), res_ty.clone()),
                };
                c::Expr::LetStrict(
                    c::Binder::new(r, c::Mult::Many, b.unboxed()),
                    Box::new(call),
                    Box::new(value),
                )
            }
        };
        let mut body = result;
        for i in (0..args.len()).rev() {
            let b = params[i].1;
            let s = self.id("s");
            if let Some(p) = self.nullable(f, &args[i], datas) {
                // let! x = case a of Nothing -> 0; Just q -> case cast q Int of I# y -> y
                let p_ty = self.ty(&p);
                let (q, y, s2) = (self.id("q"), self.id("y"), self.id("s"));
                let unbox = c::Expr::Case(
                    Box::new(c::Expr::Cast(Box::new(c::Expr::var(&q)), b.core_ty())),
                    c::Binder::new(s2, c::Mult::Many, b.core_ty()),
                    c::Type::int(),
                    vec![c::Alt {
                        con: c::AltCon::Con(
                            b.box_con().into(),
                            vec![c::Binder::new(y.clone(), c::Mult::Many, c::Type::int())],
                        ),
                        rhs: c::Expr::var(&y),
                    }],
                );
                let addr = c::Expr::Case(
                    Box::new(c::Expr::var(&arg_ids[i])),
                    c::Binder::new(s, c::Mult::Many, arg_tys[i].clone()),
                    c::Type::int(),
                    vec![
                        c::Alt {
                            con: c::AltCon::Con("Cirka.Base.Nothing".into(), Vec::new()),
                            rhs: c::Expr::Lit(c::Lit::Int(0)),
                        },
                        c::Alt {
                            con: c::AltCon::Con(
                                "Cirka.Base.Just".into(),
                                vec![c::Binder::new(q, c::Mult::Many, p_ty)],
                            ),
                            rhs: unbox,
                        },
                    ],
                );
                body = c::Expr::LetStrict(
                    c::Binder::new(raw[i].clone(), c::Mult::Many, c::Type::int()),
                    Box::new(addr),
                    Box::new(body),
                );
                continue;
            }
            body = c::Expr::Case(
                Box::new(c::Expr::Cast(
                    Box::new(c::Expr::var(&arg_ids[i])),
                    b.core_ty(),
                )),
                c::Binder::new(s, c::Mult::Many, b.core_ty()),
                res_ty.clone(),
                vec![c::Alt {
                    con: c::AltCon::Con(
                        b.box_con().into(),
                        vec![c::Binder::new(raw[i].clone(), c::Mult::Many, b.unboxed())],
                    ),
                    rhs: body,
                }],
            );
        }
        if let Some(action) = action {
            let w = self.id("w");
            let io_ty = c::Type::Con("IO".into(), vec![res_ty.clone()]);
            body = c::Expr::Cast(
                Box::new(c::Expr::Lam(
                    c::Binder::new(w, c::Mult::Many, c::Type::unit()),
                    Box::new(body),
                )),
                io_ty.clone(),
            );
            // A borrow monad of an extern block: a newtype of `IO`.
            let act_ty = self.ty(action);
            if !act_ty.alpha_eq(&io_ty) {
                body = c::Expr::Cast(Box::new(body), act_ty);
            }
        }
        for (id, t) in arg_ids.iter().zip(&arg_tys).rev() {
            body = c::Expr::Lam(
                c::Binder::new(id.clone(), c::Mult::Many, t.clone()),
                Box::new(body),
            );
        }
        let expr = self.tylams(&f.tvs, body);
        let ty = self.scheme(&f.tvs, &f.ty);
        out.binds.push(c::TopBind {
            name: f.name.clone(),
            ty,
            expr,
            span: f.span,
        });
        Ok(())
    }

    /// `foreign import ccall "&sym" p :: FunPtr ft` (or `Ptr t`): the
    /// address of a C function (with the C signature of `ft` when it has
    /// one) or of a C variable.
    #[allow(clippy::too_many_arguments)]
    fn addr_import(
        &mut self,
        f: &tast::Foreign,
        sym: &str,
        args: &[Ty],
        io: bool,
        res: &Ty,
        datas: &HashMap<&str, &tast::DataDecl>,
        out: &mut c::Program,
    ) -> Result<(), Diagnostic> {
        let ptr =
            args.is_empty() && !io && self.marshal(res, datas).is_some_and(|m| m.0 == CTy::Ptr);
        if !ptr {
            return Err(ffi_error(
                f,
                "an address import (`&name`) has type `Ptr t` or `FunPtr ft`".into(),
            ));
        }
        let sig = fun_ptr_target(res).map(|ft| {
            let (fa, _, fr) = split_fun(&ft, false);
            match self.marshal_sig(f, &fa, &fr, datas) {
                Ok(m) => scalar_sig(&m, 0),
                // A function of other types: only its address is used.
                Err(_) => c::CSig::scalars(&[], CTy::Void),
            }
        });
        let name = format!("{}$addr", f.name);
        out.externs.push(c::ExternDecl {
            name: name.clone(),
            params: Vec::new(),
            ret: c::Type::int(),
            c_sig: sig,
            link: Some(sym.to_string()),
            kind: c::ExternKind::Addr,
        });
        let r = self.id("r");
        let res_ty = self.ty(res);
        let body = c::Expr::LetStrict(
            c::Binder::new(r.clone(), c::Mult::Many, c::Type::int()),
            Box::new(c::Expr::Extern(name, Vec::new())),
            Box::new(c::Expr::Cast(
                Box::new(c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![c::Expr::var(&r)],
                )),
                res_ty,
            )),
        );
        let expr = self.tylams(&f.tvs, body);
        let ty = self.scheme(&f.tvs, &f.ty);
        out.binds.push(c::TopBind {
            name: f.name.clone(),
            ty,
            expr,
            span: f.span,
        });
        Ok(())
    }

    /// `foreign import ccall "wrapper" mk :: ft -> IO (FunPtr ft)`: a
    /// Core `:callback` extern whose environment is the Haskell function,
    /// and its handler, which boxes the C arguments, applies the function,
    /// runs the action and unboxes the result (08-ffi.md §7.2).
    #[allow(clippy::too_many_arguments)]
    fn wrapper_import(
        &mut self,
        f: &tast::Foreign,
        args: &[Ty],
        io: bool,
        res: &Ty,
        datas: &HashMap<&str, &tast::DataDecl>,
        out: &mut c::Program,
    ) -> Result<(), Diagnostic> {
        let shape = || {
            ffi_error(
                f,
                "a `wrapper` import has type `ft -> IO (FunPtr ft)` for a function type `ft`"
                    .into(),
            )
        };
        let ([ft], true, Some(pointee)) = (args, io, fun_ptr_target(res)) else {
            return Err(shape());
        };
        let ft_core = self.ty(ft);
        if !ft_core.alpha_eq(&self.ty(&pointee)) {
            return Err(shape());
        }
        if f.tvs.iter().any(|(_, k)| Ds::kind(k) != c::Kind::Type) {
            return Err(ffi_error(
                f,
                "a `wrapper` import cannot abstract over type constructors".into(),
            ));
        }
        // The handler is monomorphic; types are erased, so the type
        // variables may stand for `()`.
        let sub: Vec<(String, Ty)> = f
            .tvs
            .iter()
            .map(|(v, _)| (v.clone(), Ty::Con(unit_tc())))
            .collect();
        let mono = ft.subst(&sub);
        let (fargs, fio, fres) = split_fun(&mono, false);
        let fio = fio.is_some();
        let m = self.marshal_sig(f, &fargs, &fres, datas)?;
        let (params, ret) = (&m.0, m.1);
        let name = format!("{}$wrapper", f.name);
        let handler = format!("{}$handler", f.name);
        out.externs.push(c::ExternDecl {
            name: name.clone(),
            params: vec![c::Type::Any],
            ret: c::Type::int(),
            c_sig: Some(scalar_sig(&m, 0)),
            link: None,
            kind: c::ExternKind::Callback(handler.clone()),
        });
        // handler = \e x1 .. xn -> unbox (run ((cast e ft) (box x1) .. (box xn)))
        let e = self.id("e");
        let xs: Vec<c::Id> = (0..fargs.len())
            .map(|i| self.id(&format!("x{i}")))
            .collect();
        let mut call = c::Expr::Cast(Box::new(c::Expr::var(&e)), self.ty(&mono));
        for ((x, a), p) in xs.iter().zip(&fargs).zip(params) {
            let boxed = c::Expr::Con(p.1.box_con().into(), Vec::new(), vec![c::Expr::var(x)]);
            let a_ty = self.ty(a);
            call = c::Expr::App(
                Box::new(call),
                Box::new(c::Expr::Cast(Box::new(boxed), a_ty)),
            );
        }
        let res_m = self.ty(&fres);
        if fio {
            call = c::Expr::app(
                c::Expr::Cast(
                    Box::new(call),
                    c::Type::fun(c::Mult::Many, c::Type::unit(), res_m.clone()),
                ),
                [c::Expr::Lit(c::Lit::Unit)],
            );
        }
        let s = self.id("s");
        let (rep, body) = match ret {
            None => (
                c::Type::unit(),
                c::Expr::Case(
                    Box::new(call),
                    c::Binder::new(s, c::Mult::Many, res_m),
                    c::Type::unit(),
                    vec![c::Alt {
                        con: c::AltCon::Con("Unit".into(), Vec::new()),
                        rhs: c::Expr::Lit(c::Lit::Unit),
                    }],
                ),
            ),
            Some((_, b)) => {
                let r = self.id("r");
                (
                    b.unboxed(),
                    c::Expr::Case(
                        Box::new(c::Expr::Cast(Box::new(call), b.core_ty())),
                        c::Binder::new(s, c::Mult::Many, b.core_ty()),
                        b.unboxed(),
                        vec![c::Alt {
                            con: c::AltCon::Con(
                                b.box_con().into(),
                                vec![c::Binder::new(r.clone(), c::Mult::Many, b.unboxed())],
                            ),
                            rhs: c::Expr::var(&r),
                        }],
                    ),
                )
            }
        };
        let (mut h_expr, mut h_ty) = (body, rep);
        for (x, p) in xs.iter().zip(params).rev() {
            h_expr = c::Expr::Lam(
                c::Binder::new(x.clone(), c::Mult::Many, p.1.unboxed()),
                Box::new(h_expr),
            );
            h_ty = c::Type::fun(c::Mult::Many, p.1.unboxed(), h_ty);
        }
        out.binds.push(c::TopBind {
            name: handler,
            ty: c::Type::fun(c::Mult::Many, c::Type::Any, h_ty),
            expr: c::Expr::Lam(
                c::Binder::new(e, c::Mult::Many, c::Type::Any),
                Box::new(h_expr),
            ),
            span: f.span,
        });
        // mk = \g -> IO (\w -> let! p = extern name (cast g Any) in cast (I# p) (FunPtr ft))
        let (g, w, p) = (self.id("g"), self.id("w"), self.id("p"));
        let res_ty = self.ty(res);
        let make = c::Expr::LetStrict(
            c::Binder::new(p.clone(), c::Mult::Many, c::Type::int()),
            Box::new(c::Expr::Extern(
                name,
                vec![c::Expr::Cast(Box::new(c::Expr::var(&g)), c::Type::Any)],
            )),
            Box::new(c::Expr::Cast(
                Box::new(c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![c::Expr::var(&p)],
                )),
                res_ty.clone(),
            )),
        );
        let body = c::Expr::Lam(
            c::Binder::new(g, c::Mult::Many, ft_core),
            Box::new(c::Expr::Cast(
                Box::new(c::Expr::Lam(
                    c::Binder::new(w, c::Mult::Many, c::Type::unit()),
                    Box::new(make),
                )),
                c::Type::Con("IO".into(), vec![res_ty]),
            )),
        );
        let expr = self.tylams(&f.tvs, body);
        let ty = self.scheme(&f.tvs, &f.ty);
        out.binds.push(c::TopBind {
            name: f.name.clone(),
            ty,
            expr,
            span: f.span,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CTy, Entity, c_type_of, entity};

    #[test]
    fn c_types() {
        assert_eq!(c_type_of("Cirka.Foreign.Types.CInt"), Some(CTy::I32));
        assert_eq!(c_type_of("Cirka.Foreign.Types.Word8"), Some(CTy::U8));
        assert_eq!(c_type_of("Cirka.Base.Float"), Some(CTy::F32));
        assert_eq!(c_type_of("Cirka.Foreign.Ptr"), Some(CTy::Ptr));
        assert_eq!(c_type_of("Cirka.Prim.Int"), Some(CTy::I64));
        assert_eq!(c_type_of("Main.CInt"), None);
    }

    #[test]
    fn entity_strings() {
        let st = |n: &str| Ok(Entity::Static(n.into()));
        let addr = |n: &str| Ok(Entity::Addr(n.into()));
        assert_eq!(entity("math.h sin", "M.c_sin"), st("sin"));
        assert_eq!(entity("static stdlib.h abs", "M.f"), st("abs"));
        assert_eq!(entity("", "Main.labs"), st("labs"));
        assert_eq!(entity("&free", "M.p"), addr("free"));
        assert_eq!(entity("stdlib.h & free", "M.p"), addr("free"));
        assert_eq!(entity("&", "M.environ"), addr("environ"));
        assert_eq!(entity("dynamic", "M.p"), Ok(Entity::Dynamic));
        assert_eq!(entity(" wrapper ", "M.p"), Ok(Entity::Wrapper));
        // `static` names a C function called `dynamic`.
        assert_eq!(entity("static dynamic", "M.p"), st("dynamic"));
        assert!(entity("a b", "M.p").is_err());
        assert!(entity("&1x", "M.p").is_err());
    }
}
