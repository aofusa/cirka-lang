#!/usr/bin/env python3
"""Generates lib/cirka-base/Cirka/Foreign/Types.hs: the sized integer types
(Data.Int / Data.Word) and the C types (Foreign.C.Types) with their
instances (FEAT-0116). The instances are mechanical delegations, so they
are generated instead of written by hand.

Usage: scripts/gen-foreign-types.py  (rewrites the file in place)
"""

import os

OUT = os.path.join(os.path.dirname(__file__), "..", "lib", "cirka-base", "Cirka", "Foreign", "Types.hs")

# Sized integers: (type, constructor, bits, signed, memory kind).
# Below 64 bits the value is kept sign- or zero-extended in an Int.
SIZED = [
    ("Int8", "CirkaI8", 8, True, "cirkaMemI8"),
    ("Int16", "CirkaI16", 16, True, "cirkaMemI16"),
    ("Int32", "CirkaI32", 32, True, "cirkaMemI32"),
    ("Int64", "CirkaI64", 64, True, "cirkaMemI64"),
    ("Word8", "CirkaW8", 8, False, "cirkaMemU8"),
    ("Word16", "CirkaW16", 16, False, "cirkaMemU16"),
    ("Word32", "CirkaW32", 32, False, "cirkaMemU32"),
    ("Word64", "CirkaW64", 64, False, "cirkaMemI64"),
]

# C integer types: (type, representation).
C_INTS = [
    ("CChar", "Int8"),
    ("CSChar", "Int8"),
    ("CUChar", "Word8"),
    ("CShort", "Int16"),
    ("CUShort", "Word16"),
    ("CInt", "Int32"),
    ("CUInt", "Word32"),
    ("CLong", "Int64"),
    ("CULong", "Word64"),
    ("CLLong", "Int64"),
    ("CULLong", "Word64"),
    ("CPtrdiff", "Int64"),
    ("CSize", "Word64"),
    ("CWchar", "Int32"),
    ("CIntPtr", "Int64"),
    ("CUIntPtr", "Word64"),
    ("CIntMax", "Int64"),
    ("CUIntMax", "Word64"),
    ("CBool", "Word8"),
    ("CTime", "Int64"),
]

C_FLOATS = [("CFloat", "Float"), ("CDouble", "Double")]

SIZE = {t: b // 8 for (t, _, b, _, _) in SIZED}


def sized(t, c, bits, signed, kind):
    rep = "Int" if t != "Word64" else "Word"
    if bits == 64:
        mk = c
        norm = f"{c} x"
    elif signed:
        mk = f"cirkaTo{t}"
        norm = f"{c} (primIntShr (primIntShl x {64 - bits}) {64 - bits})"
    else:
        mk = f"cirkaTo{t}"
        norm = f"{c} (primIntAnd x {(1 << bits) - 1})"
    if signed:
        lo, hi = -(1 << (bits - 1)), (1 << (bits - 1)) - 1
        bounds = f"  minBound = {c} ({lo})\n  maxBound = {c} {hi}\n"
    elif t == "Word64":
        bounds = f"  minBound = {c} minBound\n  maxBound = {c} maxBound\n"
    else:
        bounds = f"  minBound = {c} 0\n  maxBound = {c} {(1 << bits) - 1}\n"
    s = f"-- | A {bits}-bit {'signed' if signed else 'unsigned'} integer.\nnewtype {t} = {c} {rep}\n\n"
    if bits < 64:
        s += f"{mk} :: Int -> {t}\n{mk} x = {norm}\n\n"
    # Bits: below 64 bits rotate and popCount work on the unsigned value.
    if bits == 64:
        rot = f"  rotate ({c} a) n = {c} (rotate a n)\n"
        pop = f"  popCount ({c} a) = popCount a\n"
        test = f"  testBit ({c} a) i = testBit a i\n"
    else:
        mask = (1 << bits) - 1
        rot = (
            f"  rotate ({c} a) n = case n `mod` {bits} of\n"
            f"    0 -> {c} a\n"
            f"    k -> let u = primIntAnd a {mask} in {mk} (primIntOr (primIntShl u k) (primIntShr u ({bits} - k)))\n"
        )
        pop = f"  popCount ({c} a) = popCount (primIntAnd a {mask})\n"
        test = f"  testBit ({c} a) i = i >= 0 && i < {bits} && testBit a i\n"
    if t == "Word64":
        peek = f"  peek p = fmap ({c} . CirkaWord) (cirkaPeekInt {kind} p)\n  poke p ({c} (CirkaWord x)) = cirkaPokeInt {kind} p x\n"
    else:
        peek = f"  peek p = fmap {c} (cirkaPeekInt {kind} p)\n  poke p ({c} x) = cirkaPokeInt {kind} p x\n"
    s += integral_instances(t, c, mk, bounds)
    s += (
        f"instance Bits {t} where\n"
        f"  {c} a .&. {c} b = {c} (a .&. b)\n"
        f"  {c} a .|. {c} b = {c} (a .|. b)\n"
        f"  xor ({c} a) ({c} b) = {c} (xor a b)\n"
        f"  complement ({c} a) = {mk} (complement a)\n"
        f"  shiftL ({c} a) n = {mk} (shiftL a n)\n"
        f"  shiftR ({c} a) n = {c} (shiftR a n)\n"
        f"{rot}"
        f"  rotateL x n = rotate x n\n"
        f"  rotateR x n = rotate x (negate n)\n"
        f"  zeroBits = {c} 0\n"
        f"  bit i = {mk} (bit i)\n"
        f"{test}"
        f"  bitSizeMaybe _ = Just {bits}\n"
        f"  bitSize _ = {bits}\n"
        f"  isSigned _ = {signed}\n"
        f"{pop}\n"
        f"instance FiniteBits {t} where\n  finiteBitSize _ = {bits}\n\n"
        f"instance Storable {t} where\n"
        f"  sizeOf _ = {bits // 8}\n"
        f"  alignment _ = {bits // 8}\n"
        f"{peek}\n"
    )
    return s


def integral_instances(t, c, mk, bounds):
    """Eq .. Integral of a newtype `t = c rep` whose results go through `mk`."""
    return (
        f"instance Eq {t} where\n  {c} a == {c} b = a == b\n  {c} a /= {c} b = a /= b\n\n"
        f"instance Ord {t} where\n"
        f"  compare ({c} a) ({c} b) = compare a b\n"
        f"  {c} a < {c} b = a < b\n  {c} a <= {c} b = a <= b\n"
        f"  {c} a > {c} b = a > b\n  {c} a >= {c} b = a >= b\n\n"
        f"instance Show {t} where\n  showsPrec p ({c} a) = showsPrec p a\n\n"
        f"instance Read {t} where\n"
        f"  readsPrec p s = [(fromInteger x, r) | (x, r) <- readsPrec p s]\n\n"
        f"instance Num {t} where\n"
        f"  {c} a + {c} b = {mk} (a + b)\n  {c} a - {c} b = {mk} (a - b)\n  {c} a * {c} b = {mk} (a * b)\n"
        f"  negate ({c} a) = {mk} (negate a)\n  abs ({c} a) = {mk} (abs a)\n  signum ({c} a) = {mk} (signum a)\n"
        f"  fromInteger n = {mk} (fromInteger n)\n\n"
        f"instance Real {t} where\n  toRational ({c} a) = toRational a\n\n"
        f"instance Bounded {t} where\n{bounds}\n"
        f"instance Enum {t} where\n"
        f"  succ x = x + 1\n  pred x = x - 1\n"
        f"  toEnum n = {mk} (toEnum n)\n  fromEnum ({c} a) = fromEnum a\n"
        f"  enumFrom x = enumFromTo x maxBound\n"
        f"  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)\n"
        f"  enumFromTo ({c} a) ({c} b) = map {c} (enumFromTo a b)\n"
        f"  enumFromThenTo ({c} a) ({c} b) ({c} e) = map {c} (enumFromThenTo a b e)\n\n"
        f"instance Integral {t} where\n"
        f"  quot ({c} a) ({c} b) = {mk} (quot a b)\n  rem ({c} a) ({c} b) = {mk} (rem a b)\n"
        f"  div ({c} a) ({c} b) = {mk} (div a b)\n  mod ({c} a) ({c} b) = {mk} (mod a b)\n"
        f"  quotRem x y = (quot x y, rem x y)\n  divMod x y = (div x y, mod x y)\n"
        f"  toInteger ({c} a) = toInteger a\n\n"
    )


def c_int(t, rep):
    c = t
    bounds = f"  minBound = {c} minBound\n  maxBound = {c} maxBound\n"
    return (
        f"newtype {t} = {c} {rep}\n\n"
        + integral_instances(t, c, c, bounds)
        + f"instance Bits {t} where\n"
        f"  {c} a .&. {c} b = {c} (a .&. b)\n  {c} a .|. {c} b = {c} (a .|. b)\n"
        f"  xor ({c} a) ({c} b) = {c} (xor a b)\n  complement ({c} a) = {c} (complement a)\n"
        f"  shiftL ({c} a) n = {c} (shiftL a n)\n  shiftR ({c} a) n = {c} (shiftR a n)\n"
        f"  rotate ({c} a) n = {c} (rotate a n)\n"
        f"  rotateL x n = rotate x n\n  rotateR x n = rotate x (negate n)\n"
        f"  zeroBits = {c} zeroBits\n  bit i = {c} (bit i)\n  testBit ({c} a) i = testBit a i\n"
        f"  bitSizeMaybe ({c} a) = bitSizeMaybe a\n  bitSize ({c} a) = bitSize a\n"
        f"  isSigned ({c} a) = isSigned a\n  popCount ({c} a) = popCount a\n\n"
        f"instance FiniteBits {t} where\n  finiteBitSize ({c} a) = finiteBitSize a\n\n"
        f"instance Storable {t} where\n"
        f"  sizeOf _ = {SIZE[rep]}\n  alignment _ = {SIZE[rep]}\n"
        f"  peek p = fmap {c} (peek (castPtr p))\n  poke p ({c} a) = poke (castPtr p) a\n\n"
    )


def c_float(t, rep):
    c = t
    size = 4 if rep == "Float" else 8
    un = lambda m: f"  {m} ({c} a) = {c} ({m} a)\n"
    bi = lambda m: f"  {m} ({c} a) ({c} b) = {c} ({m} a b)\n"
    return (
        f"newtype {t} = {c} {rep}\n\n"
        f"instance Eq {t} where\n  {c} a == {c} b = a == b\n  {c} a /= {c} b = a /= b\n\n"
        f"instance Ord {t} where\n"
        f"  compare ({c} a) ({c} b) = compare a b\n"
        f"  {c} a < {c} b = a < b\n  {c} a <= {c} b = a <= b\n"
        f"  {c} a > {c} b = a > b\n  {c} a >= {c} b = a >= b\n\n"
        f"instance Show {t} where\n  showsPrec p ({c} a) = showsPrec p a\n\n"
        f"instance Read {t} where\n  readsPrec p s = [({c} x, r) | (x, r) <- readsPrec p s]\n\n"
        f"instance Num {t} where\n"
        f"  {c} a + {c} b = {c} (a + b)\n  {c} a - {c} b = {c} (a - b)\n  {c} a * {c} b = {c} (a * b)\n"
        + un("negate") + un("abs") + un("signum")
        + f"  fromInteger n = {c} (fromInteger n)\n\n"
        f"instance Real {t} where\n  toRational ({c} a) = toRational a\n\n"
        f"instance Fractional {t} where\n"
        f"  {c} a / {c} b = {c} (a / b)\n" + un("recip")
        + f"  fromRational r = {c} (fromRational r)\n\n"
        f"instance Floating {t} where\n  pi = {c} pi\n"
        + "".join(un(m) for m in ["exp", "log", "sqrt", "sin", "cos", "tan", "asin", "acos", "atan",
                                    "sinh", "cosh", "tanh", "asinh", "acosh", "atanh"])
        + f"  {c} a ** {c} b = {c} (a ** b)\n" + bi("logBase") + "\n"
        f"instance RealFrac {t} where\n"
        f"  properFraction ({c} a) = case properFraction a of (n, f) -> (n, {c} f)\n"
        f"  truncate ({c} a) = truncate a\n  round ({c} a) = round a\n"
        f"  ceiling ({c} a) = ceiling a\n  floor ({c} a) = floor a\n\n"
        f"instance RealFloat {t} where\n"
        f"  floatRadix ({c} a) = floatRadix a\n  floatDigits ({c} a) = floatDigits a\n"
        f"  floatRange ({c} a) = floatRange a\n  decodeFloat ({c} a) = decodeFloat a\n"
        f"  encodeFloat m e = {c} (encodeFloat m e)\n  exponent ({c} a) = exponent a\n"
        + un("significand")
        + f"  scaleFloat n ({c} a) = {c} (scaleFloat n a)\n"
        + "".join(f"  {m} ({c} a) = {m} a\n" for m in ["isNaN", "isInfinite", "isDenormalized", "isNegativeZero", "isIEEE"])
        + bi("atan2") + "\n"
        f"instance Enum {t} where\n"
        + un("succ") + un("pred")
        + f"  toEnum n = {c} (toEnum n)\n  fromEnum ({c} a) = fromEnum a\n"
        f"  enumFrom ({c} a) = map {c} (enumFrom a)\n"
        f"  enumFromThen ({c} a) ({c} b) = map {c} (enumFromThen a b)\n"
        f"  enumFromTo ({c} a) ({c} b) = map {c} (enumFromTo a b)\n"
        f"  enumFromThenTo ({c} a) ({c} b) ({c} e) = map {c} (enumFromThenTo a b e)\n\n"
        f"instance Storable {t} where\n"
        f"  sizeOf _ = {size}\n  alignment _ = {size}\n"
        f"  peek p = fmap {c} (peek (castPtr p))\n  poke p ({c} a) = poke (castPtr p) a\n\n"
    )


def main():
    s = (
        "{-# LANGUAGE NoImplicitPrelude #-}\n"
        "-- | GENERATED by scripts/gen-foreign-types.py; do not edit.\n"
        "-- The sized integer types (Data.Int, Data.Word) and the C types\n"
        "-- (Foreign.C.Types) with their instances (FEAT-0116).\n"
        "module Cirka.Foreign.Types (module Cirka.Foreign.Types) where\n\n"
        "import Cirka.Base\nimport Cirka.Prim\nimport Cirka.Foreign\n\n"
    )
    for row in SIZED:
        s += sized(*row)
    for t, rep in C_INTS:
        s += c_int(t, rep)
    for t, rep in C_FLOATS:
        s += c_float(t, rep)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        f.write(s.rstrip("\n") + "\n")


if __name__ == "__main__":
    main()
