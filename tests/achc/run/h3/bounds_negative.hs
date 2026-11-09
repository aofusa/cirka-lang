{-# LANGUAGE BangPatterns #-}
-- ghc: skip
-- A loop from a negative index over an array whose bounds checks move to
-- the loop entry (FEAT-0069): the precondition fails on entry, so the
-- first access panics, as without the pass.
module Main where

import qualified Cirka.Array.Unboxed as A

sumA :: A.UArray Int -> Int -> Int -> Int -> Int
sumA a i n !s
  | i >= n = s
  | otherwise = sumA a (i + 1) n (s + a A.! i)

main :: IO ()
main = do
  let a = A.fromList [1, 2, 3 :: Int]
  print (sumA a 0 3 0)
  print (sumA a (-1) 3 0)
