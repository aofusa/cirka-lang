{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE KindSignatures #-}
-- | Unique arrays (FEAT-0123, 02-language-extensions.md §3.1).
--
-- An `Array e` is an immutable boxed array of lazy elements. A
-- `Unique (Array e)` is the only reference to its array, so `write` updates
-- it in place without allocating. Uniqueness is also checked at run time:
-- when the array turns out to be shared (e.g. it was captured by a thunk
-- that is still alive), `write` copies it first, so purity never depends on
-- the check alone.
--
-- `freeze` (from Cirka.Ownership) gives up uniqueness in O(1); `thaw`
-- copies unless the array is no longer shared.
module Cirka.Array
  ( Array
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
  , accum
  , lengthRef
  , index
  , writeAt
  ) where

import Cirka.Base hiding (length, read, (!), toList)
import Cirka.Ownership (Unique, unique, freeze, Ur (..))
import Unsafe.Linear (toLinear)
import Unsafe.Borrow (Ref, MutRef, Borrow, refValue, mutRefValue, borrowIO)
import Cirka.Prim (primArrayWriteShared)
import Cirka.Prim (Array, ArrayPair (..), primArrayNew, primArrayWrite, primArrayRead, primArrayLength, primArrayIndex, primArraySize, primArrayThaw)

infixl 9 !

-- | A fresh array of `n` elements, all `x`.
newArray :: Int -> e -> Unique (Array e)
newArray n x = unique (primArrayNew n x)

-- | Stores `x` at index `i` (in place). Out of bounds panics.
write :: Unique (Array e) %1 -> Int -> e -> Unique (Array e)
write u i x = toLinear (\a -> unique (primArrayWrite a i x)) (freeze u)

-- | The element at index `i` (evaluated to WHNF), and the array back.
read :: Unique (Array e) %1 -> Int -> (Unique (Array e), Ur e)
read u i = toLinear (\a -> cirkaPair (primArrayRead a i)) (freeze u)

-- | The number of elements, and the array back.
length :: Unique (Array e) %1 -> (Unique (Array e), Ur Int)
length u = toLinear (\a -> cirkaPair (primArrayLength a)) (freeze u)

cirkaPair :: ArrayPair e b -> (Unique (Array e), Ur b)
cirkaPair (ArrayPair a x) = (unique a, Ur x)

-- | Applies `f` to the element at index `i` (in place).
modify :: Unique (Array e) %1 -> Int -> (e -> e) -> Unique (Array e)
modify u i f = case read u i of
  (u', Ur x) -> write u' i (f x)

-- | Swaps the elements at indices `i` and `j` (in place).
swap :: Unique (Array e) %1 -> Int -> Int -> Unique (Array e)
swap u i j = case read u i of
  (u1, Ur x) -> case read u1 j of
    (u2, Ur y) -> write (write u2 i y) j x

-- | A unique array with the contents of `a`: `a` itself when nothing else
-- refers to it, otherwise a copy.
thaw :: Array e -> Unique (Array e)
thaw a = unique (primArrayThaw a)

-- | A unique array holding the elements of a list.
fromListU :: [e] -> Unique (Array e)
fromListU xs = case xs of
  [] -> unique (primArrayNew 0 cirkaArrayHole)
  (y : _) -> fill (newArray (cirkaListLength xs 0) y) 0 xs

fill :: Unique (Array e) %1 -> Int -> [e] -> Unique (Array e)
fill u _ [] = u
fill u i (y : ys) = fill (write u i y) (i + 1) ys

cirkaListLength :: [e] -> Int -> Int
cirkaListLength [] n = n
cirkaListLength (_ : ys) n = cirkaListLength ys (n + 1)

-- | Never evaluated: the element of an empty array.
cirkaArrayHole :: e
cirkaArrayHole = error "Cirka.Array: empty array element"

-- ---- immutable arrays ----

-- | The element at index `i`. Out of bounds panics.
(!) :: Array e -> Int -> e
(!) = primArrayIndex

-- | The number of elements.
size :: Array e -> Int
size = primArraySize

fromList :: [e] -> Array e
fromList xs = freeze (fromListU xs)

toList :: Array e -> [e]
toList a = go 0
  where
    n = size a
    go i
      | i >= n = []
      | otherwise = (a ! i) : go (i + 1)

-- | `generate n f` has `f i` at index `i`.
generate :: Int -> (Int -> e) -> Array e
generate n f
  | n <= 0 = fromList []
  | otherwise = freeze (gen (newArray n (f 0)) 1)
  where
    gen u i
      | i >= n = u
      | otherwise = gen (write u i (f i)) (i + 1)

-- | Combines the elements at the given indices with `f`, without
-- allocating a new array per update.
accum :: (e -> v -> e) -> Array e -> [(Int, v)] -> Array e
accum f a ps = freeze (go (thaw a) ps)
  where
    go u [] = u
    go u ((i, v) : rest) = go (modify u i (\x -> f x v)) rest

-- ---- borrowed arrays (Cirka.Borrow, FEAT-0124) ----

-- | The number of elements of a borrowed array.
lengthRef :: Ref r (Unique (Array e)) -> Borrow r Int
lengthRef a = borrowIO (return $! size (refValue a))

-- | The element at index `i` (evaluated to WHNF) of a borrowed array.
index :: Ref r (Unique (Array e)) -> Int -> Borrow r e
index a i = borrowIO (return $! (refValue a ! i))

-- | Stores `x` at index `i` of the owner, in place.
writeAt :: MutRef r (Unique (Array e)) -> Int -> e -> Borrow r ()
writeAt a i x = borrowIO (primArrayWriteShared (mutRefValue a) i x)
