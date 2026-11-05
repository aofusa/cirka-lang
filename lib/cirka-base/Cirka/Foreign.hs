{-# LANGUAGE NoImplicitPrelude #-}
-- | Pointers, `Bits` and `Storable` (FEAT-0116; base's Foreign.Ptr,
-- Data.Bits and Foreign.Storable). A pointer is its address as an `Int`;
-- memory is read and written through the runtime (`cirka_mem_*`). The
-- sized integer types and the C types are in Cirka.Foreign.Types.
module Cirka.Foreign
  ( module Cirka.Foreign
  ) where

import Cirka.Base
import Cirka.Prim
import Data.Char (chr, ord, intToDigit)

infixl 8 `shift`, `rotate`, `shiftL`, `shiftR`, `rotateL`, `rotateR`, `unsafeShiftL`, `unsafeShiftR`
infixl 7 .&.
infixl 6 `xor`
infixl 5 .|.

-- ---- pointers ----

newtype Ptr a = CirkaPtr Int

newtype FunPtr a = CirkaFunPtr Int

cirkaPtrAddr :: Ptr a -> Int
cirkaPtrAddr (CirkaPtr a) = a

instance Eq (Ptr a) where
  CirkaPtr a == CirkaPtr b = a == b
  CirkaPtr a /= CirkaPtr b = a /= b

instance Ord (Ptr a) where
  compare (CirkaPtr a) (CirkaPtr b) = compare (CirkaWord a) (CirkaWord b)

instance Show (Ptr a) where
  showsPrec _ (CirkaPtr a) = showString "0x" . showString (cirkaHex16 a)

instance Eq (FunPtr a) where
  CirkaFunPtr a == CirkaFunPtr b = a == b

instance Ord (FunPtr a) where
  compare (CirkaFunPtr a) (CirkaFunPtr b) = compare (CirkaWord a) (CirkaWord b)

instance Show (FunPtr a) where
  showsPrec p = showsPrec p . castFunPtrToPtr

-- | The 16 hexadecimal digits of a 64-bit word.
cirkaHex16 :: Int -> [Char]
cirkaHex16 a = go 16 a []
  where
    go 0 _ acc = acc
    go k x acc = go (k - 1) (cirkaLShr x 4) (intToDigit (primIntAnd x 15) : acc)

nullPtr :: Ptr a
nullPtr = CirkaPtr 0

castPtr :: Ptr a -> Ptr b
castPtr (CirkaPtr a) = CirkaPtr a

plusPtr :: Ptr a -> Int -> Ptr b
plusPtr (CirkaPtr a) n = CirkaPtr (a + n)

minusPtr :: Ptr a -> Ptr b -> Int
minusPtr (CirkaPtr a) (CirkaPtr b) = a - b

alignPtr :: Ptr a -> Int -> Ptr a
alignPtr p@(CirkaPtr a) n = case a `rem` n of
  0 -> p
  r -> CirkaPtr (a + (n - r))

nullFunPtr :: FunPtr a
nullFunPtr = CirkaFunPtr 0

castFunPtr :: FunPtr a -> FunPtr b
castFunPtr (CirkaFunPtr a) = CirkaFunPtr a

castFunPtrToPtr :: FunPtr a -> Ptr b
castFunPtrToPtr (CirkaFunPtr a) = CirkaPtr a

castPtrToFunPtr :: Ptr a -> FunPtr b
castPtrToFunPtr (CirkaPtr a) = CirkaFunPtr a

-- ---- bits ----

class Eq a => Bits a where
  (.&.), (.|.), xor :: a -> a -> a
  complement :: a -> a
  shift, rotate :: a -> Int -> a
  zeroBits :: a
  bit :: Int -> a
  setBit, clearBit, complementBit :: a -> Int -> a
  testBit :: a -> Int -> Bool
  bitSizeMaybe :: a -> Maybe Int
  bitSize :: a -> Int
  isSigned :: a -> Bool
  shiftL, shiftR, unsafeShiftL, unsafeShiftR :: a -> Int -> a
  rotateL, rotateR :: a -> Int -> a
  popCount :: a -> Int

  shift x n = if n >= 0 then shiftL x n else shiftR x (negate n)
  rotate x n = if n >= 0 then rotateL x n else rotateR x (negate n)
  zeroBits = clearBit (bit 0) 0
  setBit x i = x .|. bit i
  clearBit x i = x .&. complement (bit i)
  complementBit x i = x `xor` bit i
  testBit x i = (x .&. bit i) /= zeroBits
  bitSize x = case bitSizeMaybe x of
    Just n -> n
    Nothing -> errorWithoutStackTrace "Data.Bits.bitSize: no size"
  unsafeShiftL = shiftL
  unsafeShiftR = shiftR
  rotateL x n = rotate x n
  rotateR x n = rotate x (negate n)

class Bits b => FiniteBits b where
  finiteBitSize :: b -> Int
  countLeadingZeros, countTrailingZeros :: b -> Int

  countLeadingZeros x = go (finiteBitSize x - 1) 0
    where
      go i n
        | i < 0 = n
        | testBit x i = n
        | otherwise = go (i - 1) (n + 1)
  countTrailingZeros x = go 0
    where
      w = finiteBitSize x
      go i
        | i >= w = i
        | testBit x i = i
        | otherwise = go (i + 1)

toIntegralSized :: (Integral a, Integral b, Bounded b) => a -> Maybe b
toIntegralSized x =
  let y = fromIntegral x
  in if toInteger y == toInteger x then Just y else Nothing

(.^.) :: Bits a => a -> a -> a
(.^.) = xor

-- | Logical shift right of a 64-bit word by 0..63.
cirkaLShr :: Int -> Int -> Int
cirkaLShr x 0 = x
cirkaLShr x n = primIntAnd (primIntShr x n) (primIntShl 1 (64 - n) - 1)

cirkaShl :: Int -> Int -> Int
cirkaShl x n = if n >= 64 then 0 else primIntShl x n

cirkaShr :: Int -> Int -> Int
cirkaShr x n
  | n >= 64 = if x < 0 then negate 1 else 0
  | otherwise = primIntShr x n

cirkaPopCount :: Int -> Int
cirkaPopCount x = go x 0 0
  where
    go _ 64 n = n
    go y i n = go (cirkaLShr y 1) (i + 1) (n + primIntAnd y 1)

instance Bits Int where
  (.&.) = primIntAnd
  (.|.) = primIntOr
  xor = primIntXor
  complement x = primIntXor x (negate 1)
  shiftL = cirkaShl
  shiftR = cirkaShr
  rotate x n = case n `mod` 64 of
    0 -> x
    k -> primIntOr (primIntShl x k) (cirkaLShr x (64 - k))
  rotateL x n = rotate x n
  rotateR x n = rotate x (negate n)
  zeroBits = 0
  bit i = cirkaShl 1 i
  testBit x i = i >= 0 && i < 64 && primIntAnd (cirkaShr x i) 1 /= 0
  bitSizeMaybe _ = Just 64
  bitSize _ = 64
  isSigned _ = True
  popCount = cirkaPopCount

instance FiniteBits Int where
  finiteBitSize _ = 64

instance Bits Word where
  CirkaWord a .&. CirkaWord b = CirkaWord (primIntAnd a b)
  CirkaWord a .|. CirkaWord b = CirkaWord (primIntOr a b)
  xor (CirkaWord a) (CirkaWord b) = CirkaWord (primIntXor a b)
  complement (CirkaWord a) = CirkaWord (complement a)
  shiftL (CirkaWord a) n = CirkaWord (cirkaShl a n)
  shiftR (CirkaWord a) n = CirkaWord (if n >= 64 then 0 else cirkaLShr a n)
  rotate (CirkaWord a) n = CirkaWord (rotate a n)
  rotateL x n = rotate x n
  rotateR x n = rotate x (negate n)
  zeroBits = CirkaWord 0
  bit i = CirkaWord (bit i)
  testBit (CirkaWord a) i = testBit a i
  bitSizeMaybe _ = Just 64
  bitSize _ = 64
  isSigned _ = False
  popCount (CirkaWord a) = cirkaPopCount a

instance FiniteBits Word where
  finiteBitSize _ = 64

instance Bits Bool where
  (.&.) = (&&)
  (.|.) = (||)
  xor a b = a /= b
  complement = not
  shiftL x n = if n == 0 then x else False
  shiftR x n = if n == 0 then x else False
  rotate x _ = x
  zeroBits = False
  bit i = i == 0
  testBit x i = i == 0 && x
  bitSizeMaybe _ = Just 1
  bitSize _ = 1
  isSigned _ = False
  popCount x = if x then 1 else 0

instance FiniteBits Bool where
  finiteBitSize _ = 1

-- ---- storable ----

class Storable a where
  sizeOf :: a -> Int
  alignment :: a -> Int
  peekElemOff :: Ptr a -> Int -> IO a
  pokeElemOff :: Ptr a -> Int -> a -> IO ()
  peekByteOff :: Ptr b -> Int -> IO a
  pokeByteOff :: Ptr b -> Int -> a -> IO ()
  peek :: Ptr a -> IO a
  poke :: Ptr a -> a -> IO ()

  peekElemOff p i = peek (plusPtr p (i * sizeOf (cirkaPtrElem p)))
  pokeElemOff p i x = poke (plusPtr p (i * sizeOf x)) x
  peekByteOff p off = peek (plusPtr p off)
  pokeByteOff p off = poke (plusPtr p off)
  peek p = peekElemOff p 0
  poke p = pokeElemOff p 0

-- | A stand-in for the element of a pointer (only its type is used, by
-- `sizeOf` and `alignment`, which do not look at their argument).
cirkaPtrElem :: Ptr a -> a
cirkaPtrElem _ = errorWithoutStackTrace "Foreign.Storable: the element of a pointer was evaluated"

-- | Kinds of `cirka_mem_read` / `cirka_mem_write`.
cirkaMemI8, cirkaMemU8, cirkaMemI16, cirkaMemU16, cirkaMemI32, cirkaMemU32, cirkaMemI64 :: Int
cirkaMemI8 = 0
cirkaMemU8 = 1
cirkaMemI16 = 2
cirkaMemU16 = 3
cirkaMemI32 = 4
cirkaMemU32 = 5
cirkaMemI64 = 6

cirkaPeekInt :: Int -> Ptr a -> IO Int
cirkaPeekInt k p = primMemRead (cirkaPtrAddr p) k

cirkaPokeInt :: Int -> Ptr a -> Int -> IO ()
cirkaPokeInt k p x = primMemWrite (cirkaPtrAddr p) k x

instance Storable Int where
  sizeOf _ = 8
  alignment _ = 8
  peek = cirkaPeekInt cirkaMemI64
  poke = cirkaPokeInt cirkaMemI64

instance Storable Word where
  sizeOf _ = 8
  alignment _ = 8
  peek p = fmap CirkaWord (cirkaPeekInt cirkaMemI64 p)
  poke p (CirkaWord x) = cirkaPokeInt cirkaMemI64 p x

instance Storable Double where
  sizeOf _ = 8
  alignment _ = 8
  peek p = primMemReadF (cirkaPtrAddr p) 1
  poke p x = primMemWriteF (cirkaPtrAddr p) 1 x

instance Storable Float where
  sizeOf _ = 4
  alignment _ = 4
  peek p = fmap realToFrac (primMemReadF (cirkaPtrAddr p) 0)
  poke p x = primMemWriteF (cirkaPtrAddr p) 0 (realToFrac x)

instance Storable Char where
  sizeOf _ = 4
  alignment _ = 4
  peek p = fmap chr (cirkaPeekInt cirkaMemU32 p)
  poke p c = cirkaPokeInt cirkaMemU32 p (ord c)

instance Storable Bool where
  sizeOf _ = 4
  alignment _ = 4
  peek p = fmap (/= 0) (cirkaPeekInt cirkaMemI32 p)
  poke p b = cirkaPokeInt cirkaMemI32 p (if b then 1 else 0)

instance Storable (Ptr a) where
  sizeOf _ = 8
  alignment _ = 8
  peek p = fmap CirkaPtr (cirkaPeekInt cirkaMemI64 p)
  poke p (CirkaPtr x) = cirkaPokeInt cirkaMemI64 p x

instance Storable (FunPtr a) where
  sizeOf _ = 8
  alignment _ = 8
  peek p = fmap CirkaFunPtr (cirkaPeekInt cirkaMemI64 p)
  poke p (CirkaFunPtr x) = cirkaPokeInt cirkaMemI64 p x
