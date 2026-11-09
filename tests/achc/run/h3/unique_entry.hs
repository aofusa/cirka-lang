{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE BangPatterns #-}
-- ghc: skip
-- max-array-copies: 1
-- Loops entered through their unique specialization (FEAT-0070). A fresh
-- array goes straight to the specialized loop; a shared one takes the
-- original loop, whose first write makes the only copy; a loop with nothing
-- to do leaves a shared array alone.
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as A

type Arr = Unique (A.UArray Int)

fill :: Arr %1 -> Int -> Int -> Arr
fill a i n
  | i >= n = a
  | otherwise = fill (A.write a i (i * 3)) (i + 1) n

main :: IO ()
main = do
  let n = 5
      a = freeze (fill (unique (A.fromList [0, 0, 0, 0, 0 :: Int])) 0 n)
      s = A.fromList [1, 1, 1, 1, 1 :: Int]
      b = freeze (fill (unique s) 0 n)
      c = freeze (fill (unique a) n n)
  print (A.toList a)
  print (A.toList b)
  print (A.toList s)
  print (A.toList c)
