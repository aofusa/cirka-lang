//! libclang, loaded at run time (FEAT-0048, 08-ffi.md §6): the compiler
//! binaries do not link it, so only `bindgen` needs it installed.
//!
//! This module is the crate's FFI boundary and its only `unsafe` code. The
//! C API passes cursors and types by value; they point into the
//! translation unit, which [`Unit`] keeps alive for as long as the
//! [`Cursor`]s and [`Type`]s borrowed from it.

use std::ffi::{CStr, CString, c_char, c_int, c_longlong, c_uint, c_ulong, c_ulonglong, c_void};
use std::marker::PhantomData;
use std::path::Path;
use std::sync::OnceLock;

#[repr(C)]
#[derive(Clone, Copy)]
struct CXString {
    data: *const c_void,
    flags: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawCursor {
    kind: c_int,
    xdata: c_int,
    data: [*const c_void; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawType {
    kind: c_int,
    data: [*mut c_void; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CXSourceLocation {
    ptr_data: [*const c_void; 2],
    int_data: c_uint,
}

#[repr(C)]
struct CXUnsavedFile {
    filename: *const c_char,
    contents: *const c_char,
    length: c_ulong,
}

type Visitor = extern "C" fn(RawCursor, RawCursor, *mut c_void) -> c_int;

unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

const RTLD_NOW: c_int = 2;

/// Declares the libclang functions used here, with their C signatures
/// from clang-c/Index.h, and looks them up in the loaded library.
macro_rules! api {
    ($($f:ident($($a:ty),*) $(-> $r:ty)?;)*) => {
        #[allow(non_snake_case)]
        struct Api {
            $($f: unsafe extern "C" fn($($a),*) $(-> $r)?,)*
        }

        impl Api {
            /// The functions of the library `lib`.
            ///
            /// # Safety
            /// `lib` is a handle returned by `dlopen` for a libclang whose
            /// functions have the signatures above.
            unsafe fn load(lib: *mut c_void) -> Result<Api, String> {
                Ok(Api {
                    $($f: {
                        let name = concat!(stringify!($f), "\0");
                        // SAFETY: `lib` is a valid handle (caller) and the
                        // name is NUL-terminated.
                        let p = unsafe { dlsym(lib, name.as_ptr().cast()) };
                        if p.is_null() {
                            return Err(format!("libclang has no `{}`", stringify!($f)));
                        }
                        // SAFETY: the symbol is the libclang function of
                        // this name, whose C signature is the declared one.
                        unsafe {
                            std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($a),*) $(-> $r)?>(p)
                        }
                    },)*
                })
            }
        }
    };
}

api! {
    clang_createIndex(c_int, c_int) -> *mut c_void;
    clang_disposeIndex(*mut c_void);
    clang_parseTranslationUnit2(*mut c_void, *const c_char, *const *const c_char, c_int,
        *mut CXUnsavedFile, c_uint, c_uint, *mut *mut c_void) -> c_int;
    clang_disposeTranslationUnit(*mut c_void);
    clang_getNumDiagnostics(*mut c_void) -> c_uint;
    clang_getDiagnostic(*mut c_void, c_uint) -> *mut c_void;
    clang_getDiagnosticSeverity(*mut c_void) -> c_int;
    clang_formatDiagnostic(*mut c_void, c_uint) -> CXString;
    clang_disposeDiagnostic(*mut c_void);
    clang_getTranslationUnitCursor(*mut c_void) -> RawCursor;
    clang_visitChildren(RawCursor, Visitor, *mut c_void) -> c_uint;
    clang_getCursorSpelling(RawCursor) -> CXString;
    clang_getCursorUSR(RawCursor) -> CXString;
    clang_getCString(CXString) -> *const c_char;
    clang_disposeString(CXString);
    clang_getCursorType(RawCursor) -> RawType;
    clang_getTypedefDeclUnderlyingType(RawCursor) -> RawType;
    clang_getEnumDeclIntegerType(RawCursor) -> RawType;
    clang_getEnumConstantDeclValue(RawCursor) -> c_longlong;
    clang_getEnumConstantDeclUnsignedValue(RawCursor) -> c_ulonglong;
    clang_Cursor_getNumArguments(RawCursor) -> c_int;
    clang_Cursor_getArgument(RawCursor, c_uint) -> RawCursor;
    clang_Cursor_isBitField(RawCursor) -> c_uint;
    clang_getFieldDeclBitWidth(RawCursor) -> c_int;
    clang_Cursor_getOffsetOfField(RawCursor) -> c_longlong;
    clang_Cursor_isAnonymous(RawCursor) -> c_uint;
    clang_Cursor_isAnonymousRecordDecl(RawCursor) -> c_uint;
    clang_isCursorDefinition(RawCursor) -> c_uint;
    clang_getCursorDefinition(RawCursor) -> RawCursor;
    clang_Cursor_isNull(RawCursor) -> c_int;
    clang_getCursorLocation(RawCursor) -> CXSourceLocation;
    clang_getExpansionLocation(CXSourceLocation, *mut *mut c_void, *mut c_uint, *mut c_uint,
        *mut c_uint);
    clang_getFileName(*mut c_void) -> CXString;
    clang_Cursor_getStorageClass(RawCursor) -> c_int;
    clang_Cursor_isMacroFunctionLike(RawCursor) -> c_uint;
    clang_Cursor_isMacroBuiltin(RawCursor) -> c_uint;
    clang_isInvalidDeclaration(RawCursor) -> c_uint;
    clang_Cursor_Evaluate(RawCursor) -> *mut c_void;
    clang_EvalResult_getKind(*mut c_void) -> c_int;
    clang_EvalResult_isUnsignedInt(*mut c_void) -> c_uint;
    clang_EvalResult_getAsLongLong(*mut c_void) -> c_longlong;
    clang_EvalResult_getAsUnsigned(*mut c_void) -> c_ulonglong;
    clang_EvalResult_getAsDouble(*mut c_void) -> f64;
    clang_EvalResult_getAsStr(*mut c_void) -> *const c_char;
    clang_EvalResult_dispose(*mut c_void);
    clang_getCanonicalType(RawType) -> RawType;
    clang_getPointeeType(RawType) -> RawType;
    clang_getArrayElementType(RawType) -> RawType;
    clang_getArraySize(RawType) -> c_longlong;
    clang_getTypeDeclaration(RawType) -> RawCursor;
    clang_getTypeSpelling(RawType) -> CXString;
    clang_Type_getNamedType(RawType) -> RawType;
    clang_Type_getModifiedType(RawType) -> RawType;
    clang_getResultType(RawType) -> RawType;
    clang_getNumArgTypes(RawType) -> c_int;
    clang_getArgType(RawType, c_uint) -> RawType;
    clang_isFunctionTypeVariadic(RawType) -> c_uint;
    clang_Type_getSizeOf(RawType) -> c_longlong;
    clang_Type_getAlignOf(RawType) -> c_longlong;
    clang_Type_getOffsetOf(RawType, *const c_char) -> c_longlong;
}

/// Cursor kinds (`CXCursorKind`).
pub mod kind {
    pub const STRUCT: i32 = 2;
    pub const UNION: i32 = 3;
    pub const ENUM: i32 = 5;
    pub const FIELD: i32 = 6;
    pub const ENUM_CONSTANT: i32 = 7;
    pub const FUNCTION: i32 = 8;
    pub const VAR: i32 = 9;
    pub const TYPEDEF: i32 = 20;
    pub const FIRST_EXPR: i32 = 100;
    pub const STRING_LITERAL: i32 = 109;
    pub const LAST_EXPR: i32 = 199;
    pub const PACKED_ATTR: i32 = 408;
    pub const MACRO_DEFINITION: i32 = 501;
}

/// Type kinds (`CXTypeKind`).
pub mod ty {
    pub const UNEXPOSED: i32 = 1;
    pub const VOID: i32 = 2;
    pub const BOOL: i32 = 3;
    pub const CHAR_U: i32 = 4;
    pub const UCHAR: i32 = 5;
    pub const CHAR16: i32 = 6;
    pub const CHAR32: i32 = 7;
    pub const USHORT: i32 = 8;
    pub const UINT: i32 = 9;
    pub const ULONG: i32 = 10;
    pub const ULONGLONG: i32 = 11;
    pub const CHAR_S: i32 = 13;
    pub const SCHAR: i32 = 14;
    pub const WCHAR: i32 = 15;
    pub const SHORT: i32 = 16;
    pub const INT: i32 = 17;
    pub const LONG: i32 = 18;
    pub const LONGLONG: i32 = 19;
    pub const FLOAT: i32 = 21;
    pub const DOUBLE: i32 = 22;
    pub const POINTER: i32 = 101;
    pub const RECORD: i32 = 105;
    pub const ENUM: i32 = 106;
    pub const TYPEDEF: i32 = 107;
    pub const FUNCTION_NO_PROTO: i32 = 110;
    pub const FUNCTION_PROTO: i32 = 111;
    pub const CONSTANT_ARRAY: i32 = 112;
    pub const INCOMPLETE_ARRAY: i32 = 114;
    pub const VARIABLE_ARRAY: i32 = 115;
    pub const AUTO: i32 = 118;
    pub const ELABORATED: i32 = 119;
    pub const ATTRIBUTED: i32 = 163;
}

/// `CX_SC_Static`.
pub const STORAGE_STATIC: i32 = 3;

const SEVERITY_ERROR: c_int = 3;
const PARSE_DETAILED_PREPROCESSING: c_uint = 0x01;
const PARSE_SKIP_FUNCTION_BODIES: c_uint = 0x40;
const EVAL_INT: c_int = 1;
const EVAL_FLOAT: c_int = 2;
const EVAL_STR: c_int = 4;

/// A loaded libclang.
pub struct Clang {
    api: Api,
}

/// The process's libclang, loaded on first use. `path` is `CIRKA_LIBCLANG`
/// (empty: search the usual names for LLVM `version`).
pub fn load(path: Option<&str>, version: &str) -> Result<&'static Clang, String> {
    static CLANG: OnceLock<Result<Clang, String>> = OnceLock::new();
    CLANG
        .get_or_init(|| {
            let candidates: Vec<String> = match path {
                Some(p) if !p.is_empty() => vec![p.to_string()],
                _ => vec![
                    format!("libclang-{version}.so.{version}"),
                    format!("libclang-{version}.so.1"),
                    format!("libclang-{version}.so"),
                    format!("/usr/lib/llvm-{version}/lib/libclang.so.1"),
                    "libclang.so.1".to_string(),
                    "libclang.so".to_string(),
                ],
            };
            let mut last = String::new();
            for c in &candidates {
                let Ok(name) = CString::new(c.as_str()) else {
                    continue;
                };
                // SAFETY: `dlopen` reads the NUL-terminated name. Loading
                // runs libclang's constructors, as linking it would.
                let h = unsafe { dlopen(name.as_ptr(), RTLD_NOW) };
                if h.is_null() {
                    // SAFETY: `dlerror` returns a NUL-terminated message
                    // owned by the C library (or null), read right away.
                    let m = unsafe { dlerror() };
                    if !m.is_null() {
                        // SAFETY: as above.
                        last = unsafe { CStr::from_ptr(m) }.to_string_lossy().into_owned();
                    }
                    continue;
                }
                // SAFETY: `h` is a libclang just loaded; the handle is never
                // closed, so the functions stay valid for the process.
                return unsafe { Api::load(h) }.map(|api| Clang { api });
            }
            Err(format!(
                "cannot load libclang (tried {}): {last}; set CIRKA_LIBCLANG to its path",
                candidates.join(", ")
            ))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// A diagnostic of a translation unit.
pub struct Diag {
    pub error: bool,
    pub text: String,
}

/// A parsed translation unit.
pub struct Unit<'c> {
    api: &'c Api,
    index: *mut c_void,
    tu: *mut c_void,
}

impl Clang {
    /// Parses `file` (C) with the clang arguments `args`. `contents`
    /// replaces the file's contents (it need not exist). `macros` keeps the
    /// macro definitions as cursors.
    pub fn parse(
        &self,
        file: &Path,
        args: &[String],
        contents: Option<&str>,
        macros: bool,
    ) -> Result<Unit<'_>, String> {
        let name = CString::new(file.as_os_str().as_encoded_bytes())
            .map_err(|_| format!("bad file name {}", file.display()))?;
        let args: Vec<CString> = args
            .iter()
            .map(|a| CString::new(a.as_str()).map_err(|_| format!("bad clang argument `{a}`")))
            .collect::<Result<_, _>>()?;
        let argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        let text = match contents {
            Some(t) => Some(CString::new(t).map_err(|_| "NUL in generated source".to_string())?),
            None => None,
        };
        let mut unsaved = text.as_ref().map(|t| CXUnsavedFile {
            filename: name.as_ptr(),
            contents: t.as_ptr(),
            length: t.as_bytes().len() as c_ulong,
        });
        let options = PARSE_SKIP_FUNCTION_BODIES
            | if macros {
                PARSE_DETAILED_PREPROCESSING
            } else {
                0
            };
        // SAFETY: plain call; the index is disposed by `Unit`'s `Drop`.
        let index = unsafe { (self.api.clang_createIndex)(0, 0) };
        let mut tu = std::ptr::null_mut();
        let (files, nfiles) = match unsaved.as_mut() {
            Some(u) => (u as *mut CXUnsavedFile, 1),
            None => (std::ptr::null_mut(), 0),
        };
        // SAFETY: every pointer is valid for the call: the NUL-terminated
        // file name, `argc` NUL-terminated arguments, at most one unsaved
        // file whose contents has `length` bytes, and the out pointer.
        let err = unsafe {
            (self.api.clang_parseTranslationUnit2)(
                index,
                name.as_ptr(),
                argv.as_ptr(),
                argv.len() as c_int,
                files,
                nfiles,
                options,
                &mut tu,
            )
        };
        let unit = Unit {
            api: &self.api,
            index,
            tu,
        };
        if err != 0 || tu.is_null() {
            return Err(format!(
                "libclang cannot parse {} (error {err})",
                file.display()
            ));
        }
        Ok(unit)
    }
}

impl Drop for Unit<'_> {
    fn drop(&mut self) {
        // SAFETY: the unit and the index were created by `parse` and are
        // disposed once; no cursor outlives the unit (lifetimes).
        unsafe {
            if !self.tu.is_null() {
                (self.api.clang_disposeTranslationUnit)(self.tu);
            }
            (self.api.clang_disposeIndex)(self.index);
        }
    }
}

impl<'c> Unit<'c> {
    /// The unit's diagnostics, formatted as clang prints them.
    pub fn diagnostics(&self) -> Vec<Diag> {
        // SAFETY: `tu` is a live unit.
        let n = unsafe { (self.api.clang_getNumDiagnostics)(self.tu) };
        (0..n)
            .map(|i| {
                // SAFETY: `i` is in range; the diagnostic is disposed below.
                let d = unsafe { (self.api.clang_getDiagnostic)(self.tu, i) };
                // SAFETY: `d` is a live diagnostic.
                let sev = unsafe { (self.api.clang_getDiagnosticSeverity)(d) };
                // SAFETY: as above; 0xb = source location (0x1), column
                // (0x2) and the warning option (0x8), as clang prints them.
                let text = self.string(unsafe { (self.api.clang_formatDiagnostic)(d, 0xb) });
                // SAFETY: disposed once, after its last use.
                unsafe { (self.api.clang_disposeDiagnostic)(d) };
                Diag {
                    error: sev >= SEVERITY_ERROR,
                    text,
                }
            })
            .collect()
    }

    /// The cursor of the whole unit.
    pub fn cursor(&self) -> Cursor<'_> {
        // SAFETY: `tu` is a live unit.
        let raw = unsafe { (self.api.clang_getTranslationUnitCursor)(self.tu) };
        Cursor {
            raw,
            api: self.api,
            _unit: PhantomData,
        }
    }

    fn string(&self, s: CXString) -> String {
        string(self.api, s)
    }
}

fn string(api: &Api, s: CXString) -> String {
    // SAFETY: `s` is a string returned by libclang, read and then disposed
    // once.
    unsafe {
        let p = (api.clang_getCString)(s);
        let r = if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        };
        (api.clang_disposeString)(s);
        r
    }
}

/// A node of the syntax tree (a declaration, a macro definition, …).
#[derive(Clone, Copy)]
pub struct Cursor<'u> {
    raw: RawCursor,
    api: &'u Api,
    _unit: PhantomData<&'u ()>,
}

/// The value of a constant expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Eval {
    Int(i128),
    Float(f64),
    Str(Vec<u8>),
}

extern "C" fn collect(c: RawCursor, _parent: RawCursor, data: *mut c_void) -> c_int {
    // SAFETY: `data` is the `Vec` that `children` passes, alive and not
    // otherwise used during the visit.
    let out = unsafe { &mut *data.cast::<Vec<RawCursor>>() };
    out.push(c);
    1 // CXChildVisit_Continue
}

impl<'u> Cursor<'u> {
    fn wrap(&self, raw: RawCursor) -> Cursor<'u> {
        Cursor {
            raw,
            api: self.api,
            _unit: PhantomData,
        }
    }

    fn ty_of(&self, raw: RawType) -> Type<'u> {
        Type {
            raw,
            api: self.api,
            _unit: PhantomData,
        }
    }

    pub fn kind(&self) -> i32 {
        self.raw.kind
    }

    pub fn spelling(&self) -> String {
        // SAFETY: cursors are values pointing into a live unit (lifetime).
        string(self.api, unsafe {
            (self.api.clang_getCursorSpelling)(self.raw)
        })
    }

    /// The unified symbol resolution: the same declaration has the same
    /// USR in every translation unit.
    pub fn usr(&self) -> String {
        // SAFETY: as in `spelling`.
        string(self.api, unsafe { (self.api.clang_getCursorUSR)(self.raw) })
    }

    pub fn children(&self) -> Vec<Cursor<'u>> {
        let mut out: Vec<RawCursor> = Vec::new();
        // SAFETY: `collect` only pushes onto `out`, which outlives the call.
        unsafe {
            (self.api.clang_visitChildren)(
                self.raw,
                collect,
                (&mut out as *mut Vec<RawCursor>).cast(),
            )
        };
        out.into_iter().map(|c| self.wrap(c)).collect()
    }

    pub fn ty(&self) -> Type<'u> {
        // SAFETY: as in `spelling`.
        self.ty_of(unsafe { (self.api.clang_getCursorType)(self.raw) })
    }

    pub fn typedef_underlying(&self) -> Type<'u> {
        // SAFETY: as in `spelling` (an invalid type for other cursors).
        self.ty_of(unsafe { (self.api.clang_getTypedefDeclUnderlyingType)(self.raw) })
    }

    pub fn enum_int_type(&self) -> Type<'u> {
        // SAFETY: as in `spelling`.
        self.ty_of(unsafe { (self.api.clang_getEnumDeclIntegerType)(self.raw) })
    }

    /// The value of an enum constant, read as signed or unsigned.
    pub fn enum_value(&self, signed: bool) -> i128 {
        // SAFETY: as in `spelling`.
        unsafe {
            if signed {
                i128::from((self.api.clang_getEnumConstantDeclValue)(self.raw))
            } else {
                i128::from((self.api.clang_getEnumConstantDeclUnsignedValue)(self.raw))
            }
        }
    }

    /// The parameters of a function declaration.
    pub fn arguments(&self) -> Vec<Cursor<'u>> {
        // SAFETY: as in `spelling`; -1 for other cursors.
        let n = unsafe { (self.api.clang_Cursor_getNumArguments)(self.raw) };
        (0..n.max(0) as c_uint)
            // SAFETY: `i` is in range.
            .map(|i| self.wrap(unsafe { (self.api.clang_Cursor_getArgument)(self.raw, i) }))
            .collect()
    }

    /// The width of a bit-field, `None` for other fields.
    pub fn bit_width(&self) -> Option<u32> {
        // SAFETY: as in `spelling`.
        unsafe {
            if (self.api.clang_Cursor_isBitField)(self.raw) == 0 {
                return None;
            }
            u32::try_from((self.api.clang_getFieldDeclBitWidth)(self.raw)).ok()
        }
    }

    /// The offset of a field in its record, in bits.
    pub fn field_offset_bits(&self) -> Option<u64> {
        // SAFETY: as in `spelling`.
        u64::try_from(unsafe { (self.api.clang_Cursor_getOffsetOfField)(self.raw) }).ok()
    }

    /// A struct, union or enum without a name (`struct { … } x;`).
    pub fn is_anonymous(&self) -> bool {
        // SAFETY: as in `spelling`.
        unsafe { (self.api.clang_Cursor_isAnonymous)(self.raw) != 0 }
    }

    /// A C11 anonymous member (`struct { union { int a; float b; }; }`).
    pub fn is_anonymous_member(&self) -> bool {
        // SAFETY: as in `spelling`.
        unsafe { (self.api.clang_Cursor_isAnonymousRecordDecl)(self.raw) != 0 }
    }

    pub fn is_definition(&self) -> bool {
        // SAFETY: as in `spelling`.
        unsafe { (self.api.clang_isCursorDefinition)(self.raw) != 0 }
    }

    /// The definition of the declared entity, if the unit has one.
    pub fn definition(&self) -> Option<Cursor<'u>> {
        // SAFETY: as in `spelling`.
        let d = unsafe { (self.api.clang_getCursorDefinition)(self.raw) };
        // SAFETY: as above.
        (unsafe { (self.api.clang_Cursor_isNull)(d) } == 0).then(|| self.wrap(d))
    }

    /// The file and line where the declaration is (after macro expansion);
    /// `None` for built-in and command-line declarations.
    pub fn location(&self) -> Option<(String, u32)> {
        let mut file = std::ptr::null_mut();
        let mut line = 0;
        // SAFETY: as in `spelling`; the out pointers are valid (the column
        // and offset are not wanted).
        unsafe {
            let loc = (self.api.clang_getCursorLocation)(self.raw);
            (self.api.clang_getExpansionLocation)(
                loc,
                &mut file,
                &mut line,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        if file.is_null() {
            return None;
        }
        // SAFETY: `file` is a file of the live unit.
        let name = string(self.api, unsafe { (self.api.clang_getFileName)(file) });
        Some((name, line))
    }

    pub fn storage_class(&self) -> i32 {
        // SAFETY: as in `spelling`.
        unsafe { (self.api.clang_Cursor_getStorageClass)(self.raw) }
    }

    /// A macro definition of the form `#define F(x) …` or a compiler's.
    pub fn is_function_like_or_builtin_macro(&self) -> bool {
        // SAFETY: as in `spelling`.
        unsafe {
            (self.api.clang_Cursor_isMacroFunctionLike)(self.raw) != 0
                || (self.api.clang_Cursor_isMacroBuiltin)(self.raw) != 0
        }
    }

    pub fn is_invalid(&self) -> bool {
        // SAFETY: as in `spelling`.
        unsafe { (self.api.clang_isInvalidDeclaration)(self.raw) != 0 }
    }

    /// The value of a variable's initializer (or of an expression), when
    /// clang can evaluate it to an integer, a floating-point number or a
    /// string literal.
    pub fn evaluate(&self) -> Option<Eval> {
        // SAFETY: as in `spelling`; the result is disposed below.
        let e = unsafe { (self.api.clang_Cursor_Evaluate)(self.raw) };
        if e.is_null() {
            return None;
        }
        // SAFETY: `e` is a live result, disposed once after its last use;
        // the string is copied before that.
        unsafe {
            let r = match (self.api.clang_EvalResult_getKind)(e) {
                EVAL_INT => Some(Eval::Int(
                    if (self.api.clang_EvalResult_isUnsignedInt)(e) != 0 {
                        i128::from((self.api.clang_EvalResult_getAsUnsigned)(e))
                    } else {
                        i128::from((self.api.clang_EvalResult_getAsLongLong)(e))
                    },
                )),
                EVAL_FLOAT => Some(Eval::Float((self.api.clang_EvalResult_getAsDouble)(e))),
                EVAL_STR => {
                    let p = (self.api.clang_EvalResult_getAsStr)(e);
                    (!p.is_null()).then(|| Eval::Str(CStr::from_ptr(p).to_bytes().to_vec()))
                }
                _ => None,
            };
            (self.api.clang_EvalResult_dispose)(e);
            r
        }
    }
}

/// A C type.
#[derive(Clone, Copy)]
pub struct Type<'u> {
    raw: RawType,
    api: &'u Api,
    _unit: PhantomData<&'u ()>,
}

impl<'u> Type<'u> {
    fn wrap(&self, raw: RawType) -> Type<'u> {
        Type {
            raw,
            api: self.api,
            _unit: PhantomData,
        }
    }

    pub fn kind(&self) -> i32 {
        self.raw.kind
    }

    pub fn canonical(&self) -> Type<'u> {
        // SAFETY: types are values pointing into a live unit (lifetime).
        self.wrap(unsafe { (self.api.clang_getCanonicalType)(self.raw) })
    }

    pub fn pointee(&self) -> Type<'u> {
        // SAFETY: as in `canonical`.
        self.wrap(unsafe { (self.api.clang_getPointeeType)(self.raw) })
    }

    pub fn element(&self) -> Type<'u> {
        // SAFETY: as in `canonical`.
        self.wrap(unsafe { (self.api.clang_getArrayElementType)(self.raw) })
    }

    pub fn array_size(&self) -> Option<u64> {
        // SAFETY: as in `canonical`.
        u64::try_from(unsafe { (self.api.clang_getArraySize)(self.raw) }).ok()
    }

    /// The declaration of a record, enum or typedef type.
    pub fn declaration(&self) -> Cursor<'u> {
        Cursor {
            // SAFETY: as in `canonical`.
            raw: unsafe { (self.api.clang_getTypeDeclaration)(self.raw) },
            api: self.api,
            _unit: PhantomData,
        }
    }

    pub fn spelling(&self) -> String {
        // SAFETY: as in `canonical`.
        string(self.api, unsafe {
            (self.api.clang_getTypeSpelling)(self.raw)
        })
    }

    /// The type an elaborated type (`struct S`, a qualified name) names.
    pub fn named(&self) -> Type<'u> {
        // SAFETY: as in `canonical`.
        self.wrap(unsafe { (self.api.clang_Type_getNamedType)(self.raw) })
    }

    /// The type an attributed type (`__attribute__((…)) T`) modifies.
    pub fn modified(&self) -> Type<'u> {
        // SAFETY: as in `canonical`.
        self.wrap(unsafe { (self.api.clang_Type_getModifiedType)(self.raw) })
    }

    pub fn result(&self) -> Type<'u> {
        // SAFETY: as in `canonical`.
        self.wrap(unsafe { (self.api.clang_getResultType)(self.raw) })
    }

    /// The parameter types of a function type (after array and function
    /// parameters decay to pointers).
    pub fn arg_types(&self) -> Vec<Type<'u>> {
        // SAFETY: as in `canonical`; -1 for types without a prototype.
        let n = unsafe { (self.api.clang_getNumArgTypes)(self.raw) };
        (0..n.max(0) as c_uint)
            // SAFETY: `i` is in range.
            .map(|i| self.wrap(unsafe { (self.api.clang_getArgType)(self.raw, i) }))
            .collect()
    }

    pub fn is_variadic(&self) -> bool {
        // SAFETY: as in `canonical`.
        unsafe { (self.api.clang_isFunctionTypeVariadic)(self.raw) != 0 }
    }

    /// Size in bytes (`None` for incomplete or invalid types).
    pub fn size(&self) -> Option<u64> {
        // SAFETY: as in `canonical`.
        u64::try_from(unsafe { (self.api.clang_Type_getSizeOf)(self.raw) }).ok()
    }

    /// Alignment in bytes.
    pub fn align(&self) -> Option<u64> {
        // SAFETY: as in `canonical`.
        u64::try_from(unsafe { (self.api.clang_Type_getAlignOf)(self.raw) }).ok()
    }

    /// The offset in bits of the field `name` of a record type, also
    /// through anonymous members.
    pub fn offset_of(&self, name: &str) -> Option<u64> {
        let n = CString::new(name).ok()?;
        // SAFETY: as in `canonical`; the name is NUL-terminated.
        u64::try_from(unsafe { (self.api.clang_Type_getOffsetOf)(self.raw, n.as_ptr()) }).ok()
    }
}
