{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE BangPatterns #-}
-- ghc: skip
-- max-array-copies: 3
-- Loops specialized for unique arrays (FEAT-0067) keep value semantics when
-- the array is shared. `fill` starts on a shared array and skips some
-- iterations: those pass the array unchecked, so they must not enter the
-- specialized function; the first write copies and the rest of the loop runs
-- without checks. `snapshots` shares the array in the middle of the loop, so
-- it has to leave the specialized function and copy again. The three copies
-- are one in `fill` and two in `snapshots`; a `fill` that writes nothing
-- copies nothing.
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as A

fill :: Unique (A.UArray Int) %1 -> Int -> Int -> Int -> Unique (A.UArray Int)
fill u i k n
  | i >= n = u
  | i < k = fill u (i + 1) k n
  | otherwise = fill (A.write u i (i * i)) (i + 1) k n

set0 :: A.UArray Int -> Int -> A.UArray Int
set0 a x = freeze (A.write (unique a) 0 x)

snapshots :: A.UArray Int -> A.UArray Int -> Int -> Int -> (A.UArray Int, A.UArray Int)
snapshots !u s i n
  | i >= n = (u, s)
  | i == 3 = let a = set0 u i in a `seq` snapshots a a (i + 1) n
  | otherwise = snapshots (set0 u i) s (i + 1) n

main :: IO ()
main = do
  let a = A.fromList [1, 2, 3, 4, 5, 6, 7, 8 :: Int]
      b = freeze (fill (unique a) 0 2 8)
      c = freeze (fill (unique a) 0 8 8)
  print (A.toList b)
  print (A.toList c)
  case snapshots a a 0 6 of
    (u, s) -> do
      print (A.toList u)
      print (A.toList s)
  print (A.toList a)
