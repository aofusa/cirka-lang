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

/// The C name of `foreign import ccall "[static] [header.h] [name]"`.
fn c_name(entity: &str, hs_name: &str) -> Result<String, String> {
    let mut toks: Vec<&str> = entity.split_whitespace().collect();
    if toks.first() == Some(&"static") {
        toks.remove(0);
    }
    if toks.first().is_some_and(|t| t.ends_with(".h")) {
        toks.remove(0);
    }
    match toks.as_slice() {
        [] => Ok(hs_name.rsplit('.').next().unwrap_or(hs_name).to_string()),
        ["dynamic"] | ["wrapper"] => Err(format!(
            "`{}` imports (calling and creating function pointers) are not supported yet",
            toks[0]
        )),
        [n] if n.starts_with('&') => {
            Err("address imports (`&name`) are not supported yet".to_string())
        }
        [n] if n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !n.starts_with(|c: char| c.is_ascii_digit()) =>
        {
            Ok(n.to_string())
        }
        _ => Err(format!("`{entity}` is not a C function name")),
    }
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

    /// Desugars one `foreign import ccall`.
    pub(crate) fn ccall(
        &mut self,
        f: &tast::Foreign,
        datas: &HashMap<&str, &tast::DataDecl>,
        out: &mut c::Program,
    ) -> Result<(), Diagnostic> {
        let fail = |msg: String| {
            Diagnostic::error(msg)
                .with_code("E0618")
                .with_label(f.span, "")
        };
        let name = c_name(&f.entity, &f.name).map_err(|m| {
            fail(m).with_note("only static imports of C functions are available (FEAT-0047)")
        })?;
        // Split the type into the arguments and the (IO) result.
        let mut args = Vec::new();
        let mut t = f.ty.clone();
        while let Ty::Fun(a, r) = t {
            args.push(*a);
            t = *r;
        }
        let (io, res) = match t.spine() {
            (Ty::Con(h), a) if *h == prim("IO") && a.len() == 1 => (true, a[0].clone()),
            _ => (false, t.clone()),
        };
        let mut params = Vec::new();
        for a in &args {
            match self.marshal(a, datas) {
                Some(m) => params.push(m),
                None => {
                    return Err(
                        fail(format!("`{}` cannot be passed to a C function", show(a))).with_note(
                            "marshallable types are Int, Word, Double, Float, Char, Int8..Word64, \
                         the C types of Foreign.C.Types, Ptr and FunPtr, and newtypes of them",
                        ),
                    );
                }
            }
        }
        let ret = if res == Ty::Con(unit_tc()) {
            None
        } else {
            match self.marshal(&res, datas) {
                Some(m) => Some(m),
                None => {
                    return Err(fail(format!(
                        "`{}` cannot be returned from a C function",
                        show(&res)
                    )));
                }
            }
        };
        let sig = c::CSig {
            params: params.iter().map(|p| p.0).collect(),
            ret: ret.map_or(CTy::Void, |r| r.0),
        };
        let decl = c::ExternDecl {
            name: name.clone(),
            params: params.iter().map(|p| p.1.unboxed()).collect(),
            ret: ret.map_or(c::Type::unit(), |r| r.1.unboxed()),
            c_sig: Some(sig),
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
            Some(c::CSig {
                params: ps?,
                ret: conv(&e.ret)?,
            })
        };
        match out.externs.iter().find(|e| e.name == name) {
            Some(e) if e.c_sig.is_none() && plain(e) == decl.c_sig => {}
            Some(e) if *e != decl => {
                return Err(fail(format!(
                    "`{name}` is imported with two different C types"
                )));
            }
            Some(_) => {}
            None => out.externs.push(decl),
        }
        // \a1 .. an -> [\w ->] case a1 of I# x1 -> .. let! r = extern name x1 .. in box r
        let arg_tys: Vec<c::Type> = args.iter().map(|a| self.ty(a)).collect();
        let res_ty = self.ty(&res);
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
                c::Expr::LetStrict(
                    c::Binder::new(r, c::Mult::Many, b.unboxed()),
                    Box::new(call),
                    Box::new(c::Expr::Cast(Box::new(boxed), res_ty.clone())),
                )
            }
        };
        let mut body = result;
        for i in (0..args.len()).rev() {
            let b = params[i].1;
            let s = self.id("s");
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
        if io {
            let w = self.id("w");
            let io_ty = c::Type::Con("IO".into(), vec![res_ty.clone()]);
            body = c::Expr::Cast(
                Box::new(c::Expr::Lam(
                    c::Binder::new(w, c::Mult::Many, c::Type::unit()),
                    Box::new(body),
                )),
                io_ty,
            );
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
}

#[cfg(test)]
mod tests {
    use super::{CTy, c_name, c_type_of};

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
        assert_eq!(c_name("math.h sin", "M.c_sin").unwrap(), "sin");
        assert_eq!(c_name("static stdlib.h abs", "M.f").unwrap(), "abs");
        assert_eq!(c_name("", "Main.labs").unwrap(), "labs");
        assert!(c_name("&free", "M.p").is_err());
        assert!(c_name("dynamic", "M.p").is_err());
        assert!(c_name("a b", "M.p").is_err());
    }
}
