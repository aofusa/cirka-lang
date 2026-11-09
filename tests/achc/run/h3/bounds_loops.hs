{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE BangPatterns #-}
-- ghc: skip
-- Loops whose bounds checks move to the loop entry (FEAT-0069). The
-- precondition holds (fill, rev, sumA), or fails on entry and the loop runs
-- checked: `find` searches past the end of the array but stops at the
-- element first, `sumA` sums a prefix from 1.
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as A

type Arr = Unique (A.UArray Int)

fill :: Arr %1 -> Int -> Int -> Arr
fill a i n
  | i >= n = a
  | otherwise = fill (A.write a i (i * i)) (i + 1) n

rev :: Arr %1 -> Int -> Int -> Arr
rev p lo hi
  | lo >= hi = p
  | otherwise = rev (A.swap p lo hi) (lo + 1) (hi - 1)

-- The index of `x` in a[i .. n - 1], or -1.
find :: Arr %1 -> Int -> Int -> Int -> (Arr, Ur Int)
find a i n x
  | i >= n = (a, Ur (-1))
  | otherwise = case A.read a i of
      (a', Ur y) -> if y == x then (a', Ur i) else find a' (i + 1) n x

sumA :: A.UArray Int -> Int -> Int -> Int -> Int
sumA a i n !s
  | i >= n = s
  | otherwise = sumA a (i + 1) n (s + a A.! i)

found :: (Arr, Ur Int) %1 -> Int
found (a, Ur k) = k * 100 + A.size (freeze a)

main :: IO ()
main = do
  let a = freeze (rev (fill (A.newArray 10 0) 0 10) 0 9)
  print (A.toList a)
  print (sumA a 0 10 0)
  print (sumA a 1 4 0)
  print (found (find (fill (A.newArray 10 0) 0 10) 0 20 49))
  print (found (find (fill (A.newArray 10 0) 0 10) 2 5 49))
