{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE BangPatterns #-}
-- | Unboxed unique arrays (FEAT-0123): the API of Cirka.Array for elements
-- of class `UElem` (`Int`, `Double`, `Char`, `Bool`).
--
-- A `UArray e` is a BYTES object of 8-byte words. Elements are strict: a
-- write evaluates the element. As with Cirka.Array, `write` on a
-- `Unique (UArray e)` is in place, and copies first when the array turns out
-- to be shared at run time.
module Cirka.Array.Unboxed
  ( UArray
  , UElem
  , newArray
  , write
  , read
  , length
  , modify
  , swap
  , thaw
  , fromListU
  , (!)
  , size
  , fromList
  , toList
  , generate
  , lengthRef
  , index
  , writeAt
  ) where

import Cirka.Base hiding (length, read, (!), toList)
import Cirka.Ownership (Unique, unique, freeze, Ur (..))
import Cirka.Prim (Bytes, BytesPair (..), primDoubleBits, primUArrNew, primUArrSize, primUArrSizeU, primUArrThaw, primUArrIndexInt, primUArrReadInt, primUArrWriteInt, primUArrIndexDbl, primUArrReadDbl, primUArrWriteDbl, primUArrWriteIntShared, primUArrWriteDblShared)
import Unsafe.Linear (toLinear)
import Unsafe.Borrow (Ref, MutRef, Borrow, refValue, mutRefValue, borrowIO)

infixl 9 !

-- Operations take the BYTES object apart by pattern matching (not through
-- a function), and evaluate the results of the (unknown) `UElem` methods
-- with bang patterns, so that no thunk holds a second reference to it.
newtype UArray e = UArray Bytes

-- | Element types stored unboxed (one 8-byte word each).
class UElem e where
  cirkaUIndex :: Bytes -> Int -> e
  cirkaURead :: Bytes -> Int -> BytesPair e
  cirkaUWrite :: Bytes -> Int -> e -> Bytes
  -- | In-place store through a mutable borrow.
  cirkaUWriteShared :: Bytes -> Int -> e -> IO ()
  -- | Whether the element is stored as zero bits, as in a fresh array.
  cirkaUZero :: e -> Bool

instance UElem Int where
  cirkaUIndex = primUArrIndexInt
  cirkaURead = primUArrReadInt
  cirkaUWrite = primUArrWriteInt
  cirkaUWriteShared = primUArrWriteIntShared
  cirkaUZero x = x == 0

instance UElem Double where
  cirkaUIndex = primUArrIndexDbl
  cirkaURead = primUArrReadDbl
  cirkaUWrite = primUArrWriteDbl
  cirkaUWriteShared = primUArrWriteDblShared
  -- By its bits: `-0.0 == 0` holds, but it is not stored as zero bits.
  cirkaUZero x = primDoubleBits x == 0

instance UElem Char where
  cirkaUIndex b i = toEnum (primUArrIndexInt b i)
  cirkaURead b i = case primUArrReadInt b i of
    BytesPair b' n -> BytesPair b' (toEnum n)
  cirkaUWrite b i c = primUArrWriteInt b i (fromEnum c)
  cirkaUWriteShared b i c = primUArrWriteIntShared b i (fromEnum c)
  cirkaUZero c = fromEnum c == 0

instance UElem Bool where
  cirkaUIndex b i = primUArrIndexInt b i /= 0
  cirkaURead b i = case primUArrReadInt b i of
    BytesPair b' n -> BytesPair b' (n /= 0)
  cirkaUWrite b i x = primUArrWriteInt b i (if x then 1 else 0)
  cirkaUWriteShared b i x = primUArrWriteIntShared b i (if x then 1 else 0)
  cirkaUZero x = not x

-- | A fresh array of `n` elements, all `x`. A fresh array is zeroed, so an
-- element of zero bits needs no writes. Without elements, `x` stays
-- unevaluated.
newArray :: UElem e => Int -> e -> Unique (UArray e)
newArray n x
  | n > 0 && cirkaUZero x = unique (UArray (primUArrNew n))
  | otherwise = fill0 (unique (UArray (primUArrNew n))) 0
  where
    fill0 u i
      | i >= n = u
      | otherwise = fill0 (write u i x) (i + 1)

-- | Stores `x` (evaluated) at index `i` (in place). Out of bounds panics.
write :: UElem e => Unique (UArray e) %1 -> Int -> e -> Unique (UArray e)
write u i x = toLinear (\(UArray b) -> let !b' = cirkaUWrite b i x in unique (UArray b')) (freeze u)

-- | The element at index `i`, and the array back.
read :: UElem e => Unique (UArray e) %1 -> Int -> (Unique (UArray e), Ur e)
read u i = toLinear (\(UArray b) -> let !p = cirkaURead b i in cirkaPair p) (freeze u)

-- | The number of elements, and the array back.
length :: Unique (UArray e) %1 -> (Unique (UArray e), Ur Int)
length u = toLinear (\(UArray b) -> cirkaPair (primUArrSizeU b)) (freeze u)

cirkaPair :: BytesPair b -> (Unique (UArray e), Ur b)
cirkaPair (BytesPair a x) = (unique (UArray a), Ur x)

-- | Applies `f` to the element at index `i` (in place).
modify :: UElem e => Unique (UArray e) %1 -> Int -> (e -> e) -> Unique (UArray e)
modify u i f = case read u i of
  (u', Ur x) -> write u' i (f x)

-- | Swaps the elements at indices `i` and `j` (in place).
swap :: UElem e => Unique (UArray e) %1 -> Int -> Int -> Unique (UArray e)
swap u i j = case read u i of
  (u1, Ur x) -> case read u1 j of
    (u2, Ur y) -> write (write u2 i y) j x

-- | A unique array with the contents of `a`: `a` itself when nothing else
-- refers to it, otherwise a copy.
thaw :: UArray e -> Unique (UArray e)
thaw (UArray b) = unique (UArray (primUArrThaw b))

-- | A unique array holding the elements of a list.
fromListU :: UElem e => [e] -> Unique (UArray e)
fromListU xs = fill (unique (UArray (primUArrNew (count xs 0)))) 0 xs
  where
    count [] n = n
    count (_ : ys) n = count ys (n + 1)
    fill u _ [] = u
    fill u i (y : ys) = fill (write u i y) (i + 1) ys

-- ---- immutable arrays ----

-- | The element at index `i`. Out of bounds panics.
(!) :: UElem e => UArray e -> Int -> e
UArray b ! i = cirkaUIndex b i

-- | The number of elements.
size :: UArray e -> Int
size (UArray b) = primUArrSize b

fromList :: UElem e => [e] -> UArray e
fromList xs = freeze (fromListU xs)

toList :: UElem e => UArray e -> [e]
toList a = go 0
  where
    n = size a
    go i
      | i >= n = []
      | otherwise = (a ! i) : go (i + 1)

-- | `generate n f` has `f i` at index `i`.
generate :: UElem e => Int -> (Int -> e) -> UArray e
generate n f = freeze (gen (unique (UArray (primUArrNew n))) 0)
  where
    gen u i
      | i >= n = u
      | otherwise = gen (write u i (f i)) (i + 1)

-- ---- borrowed arrays (Cirka.Borrow, FEAT-0124) ----

-- | The number of elements of a borrowed array.
lengthRef :: Ref r (Unique (UArray e)) -> Borrow r Int
lengthRef a = borrowIO (return $! size (refValue a))

-- | The element at index `i` of a borrowed array.
index :: UElem e => Ref r (Unique (UArray e)) -> Int -> Borrow r e
index a i = borrowIO (return $! (refValue a ! i))

-- | Stores `x` (evaluated) at index `i` of the owner, in place.
writeAt :: UElem e => MutRef r (Unique (UArray e)) -> Int -> e -> Borrow r ()
writeAt a i x = case mutRefValue a of
  UArray b -> borrowIO (cirkaUWriteShared b i x)
