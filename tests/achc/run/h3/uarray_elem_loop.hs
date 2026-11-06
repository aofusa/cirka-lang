{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE BangPatterns #-}
-- ghc: skip
-- opt-levels: 2
-- max-allocs: 2000
-- max-array-copies: 0
-- Loops over `UArray Double` / `UArray Int` (FEAT-0059): the `UElem`
-- methods are specialized at the element type and inlined, so writing and
-- reading elements allocates nothing per element (100,000 elements here).
-- (`generate` still boxes through its unknown element function: FEAT-0061.)
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as U

n :: Int
n = 100000

sumD :: U.UArray Double -> Double
sumD a = go 0 0
  where
    go i !s
      | i >= n = s
      | otherwise = go (i + 1) (s + a U.! i)

sumI :: U.UArray Int -> Int
sumI a = go 0 0
  where
    go i !s
      | i >= n = s
      | otherwise = go (i + 1) (s + a U.! i)

fillD :: Unique (U.UArray Double) %1 -> Int -> Unique (U.UArray Double)
fillD a i
  | i >= n = a
  | otherwise = fillD (U.write a i (fromIntegral i * 0.5)) (i + 1)

fillI :: Unique (U.UArray Int) %1 -> Int -> Unique (U.UArray Int)
fillI a i
  | i >= n = a
  | otherwise = fillI (U.write a i (i * 3)) (i + 1)

main :: IO ()
main = do
  let d = freeze (fillD (U.newArray n 0) 0)
      k = freeze (fillI (U.newArray n 0) 0)
  print (sumD d)
  print (sumI k)
