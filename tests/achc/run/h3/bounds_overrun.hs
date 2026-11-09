{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- A loop past the end of an array whose bounds checks move to the loop
-- entry (FEAT-0069): the precondition fails on entry, so the loop runs
-- checked and panics at the first index out of bounds, as without the pass.
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as A

fill :: Unique (A.UArray Int) %1 -> Int -> Int -> Unique (A.UArray Int)
fill a i n
  | i >= n = a
  | otherwise = fill (A.write a i i) (i + 1) n

main :: IO ()
main = do
  print (A.size (freeze (fill (A.newArray 4 0) 0 4)))
  print (A.size (freeze (fill (A.newArray 4 0) 0 5)))
