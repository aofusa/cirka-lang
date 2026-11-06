{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE BangPatterns #-}
-- ghc: skip
-- opt-levels: 2
-- max-allocs: 1000
-- max-array-copies: 0
-- fannkuch-redux (n = 7) over unique unboxed arrays, as bench/fannkuch-redux/own.chs
-- (FEAT-0064, FEAT-0065): no allocation per permutation (the tuples returned
-- by copy/flips/next are multiple results; 143 allocations in all, about
-- 270,000 before FEAT-0064).
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as A
import System.Environment (getArgs)

type Arr = Unique (A.UArray Int)

-- count[i - 1] = i for i = r down to 2.
fill :: Arr %1 -> Int -> Arr
fill c r
  | r <= 1 = c
  | otherwise = fill (A.write c (r - 1) r) (r - 1)

-- perm[0 .. n - 1] = perm1[0 .. n - 1].
copy :: Arr %1 -> Arr %1 -> Int -> Int -> (Arr, Arr)
copy src dst i n
  | i >= n = (src, dst)
  | otherwise = case A.read src i of
      (src', Ur x) -> copy src' (A.write dst i x) (i + 1) n

-- Reverses perm[lo .. hi].
rev :: Arr %1 -> Int -> Int -> Arr
rev p lo hi
  | lo >= hi = p
  | otherwise = rev (A.swap p lo hi) (lo + 1) (hi - 1)

-- Prefix reversals until the first element is 0.
flips :: Arr %1 -> Int -> (Arr, Ur Int)
flips p f = case A.read p 0 of
  (p', Ur k) -> if k == 0 then (p', Ur f) else flips (rev p' 0 k) (f + 1)

-- perm1[i] = perm1[i + 1] for i = 0 .. r - 1.
shift :: Arr %1 -> Int -> Int -> Arr
shift p i r
  | i >= r = p
  | otherwise = case A.read p (i + 1) of
      (p', Ur x) -> shift (A.write p' i x) (i + 1) r

-- Rotates perm1[0 .. r] while count[r] reaches 0; the new r (n: done).
next :: Int -> Arr %1 -> Arr %1 -> Int -> (Arr, Arr, Ur Int)
next n p1 c r
  | r == n = (p1, c, Ur r)
  | otherwise = case A.read p1 0 of
      (p1a, Ur p0) -> case A.read c r of
        (c1, Ur cr) ->
          nextIf n (A.write (shift p1a 0 r) r p0) (A.write c1 r (cr - 1)) r (cr - 1 > 0)

nextIf :: Int -> Arr %1 -> Arr %1 -> Int -> Bool -> (Arr, Arr, Ur Int)
nextIf n p1 c r True = (p1, c, Ur r)
nextIf n p1 c r False = next n p1 c (r + 1)

loop :: Int -> Arr %1 -> Arr %1 -> Arr %1 -> Int -> Int -> Int -> Int -> (Int, Int)
loop n p1 p c r !pc !cs !mf = case copy p1 p 0 n of
  (p1a, pa) -> case flips pa 0 of
    (pb, Ur f) ->
      let cs' = if even pc then cs + f else cs - f
          mf' = max mf f
       in case next n p1a (fill c r) 1 of
            (p1b, cb, Ur r') ->
              if r' == n
                then dropThen p1b (dropThen pb (dropThen cb (cs', mf')))
                else loop n p1b pb cb r' (pc + 1) cs' mf'

main :: IO ()
main = do
  args <- getArgs
  let n = case args of { (s : _) -> read s; [] -> 7 }
      (checksum, f) = loop n (A.fromListU [0 .. n - 1]) (A.newArray n 0) (A.newArray n 0) n 0 0 0
  putStrLn (show checksum ++ "\nPfannkuchen(" ++ show n ++ ") = " ++ show f)
