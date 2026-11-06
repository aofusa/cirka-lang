{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE BangPatterns #-}
-- ghc: skip
-- opt-levels: 2
-- max-allocs: 1000
-- max-array-copies: 0
-- In-place quicksort of 20,000 elements, as bench/array-sort/own.chs
-- (FEAT-0066): the check loop reads each element without boxing it (one
-- box per element, about 20,000 allocations, before).
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as A

-- Pseudo-random input (LCG); the same sequence as plain.hs and main.rs.
build :: Int -> Unique (A.UArray Int)
build n = go (A.newArray n 0) 0 12345
  where
    go u i s
      | i >= n = u
      | otherwise =
          let s' = (s * 1103515245 + 12345) `rem` 2147483648
           in go (A.write u i (s' `rem` 1000000)) (i + 1) s'

-- Lomuto partition of [lo, hi] around the last element.
partition :: Unique (A.UArray Int) %1 -> Int -> Int -> (Unique (A.UArray Int), Ur Int)
partition u0 lo hi = case A.read u0 hi of
  (u1, Ur p) -> loop u1 lo lo
    where
      loop u i j
        | j >= hi = (A.swap u i hi, Ur i)
        | otherwise = case A.read u j of
            (u', Ur x) ->
              if x < p
                then loop (A.swap u' i j) (i + 1) (j + 1)
                else loop u' i (j + 1)

qsort :: Unique (A.UArray Int) %1 -> Int -> Int -> Unique (A.UArray Int)
qsort u lo hi
  | lo >= hi = u
  | otherwise = case partition u lo hi of
      (u', Ur m) -> qsort (qsort u' lo (m - 1)) (m + 1) hi

-- Checks the order and sums a sample of the elements.
check :: A.UArray Int -> Int -> Int -> Int -> Bool -> (Int, Bool)
check a n i !acc ok
  | i >= n = (acc, ok)
  | otherwise =
      let x = a A.! i
          ok' = ok && (i == 0 || a A.! (i - 1) <= x)
       in ok' `seq` check a n (i + 1) ((acc + x * (i `rem` 7)) `rem` 1000000007) ok'

main :: IO ()
main = do
  let s = "20000"
  let n = read s :: Int
      a = freeze (qsort (build n) 0 (n - 1))
  print (check a n 0 0 True)
