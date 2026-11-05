-- In-place quicksort with GHC's STUArray (the GHC -O2 baseline of own.chs;
-- aCHC has no Data.Array.ST, so only GHC builds this one).
-- achc: skip
module Main where

import Control.Monad (when)
import Control.Monad.ST (ST)
import Data.Array.ST (STUArray, newArray, readArray, writeArray, runSTUArray)
import Data.Array.Unboxed (UArray, (!))
import System.Environment (getArgs)

build :: Int -> ST s (STUArray s Int Int)
build n = do
  a <- newArray (0, n - 1) 0
  let go i s = when (i < n) $ do
        let s' = (s * 1103515245 + 12345) `rem` 2147483648
        writeArray a i (s' `rem` 1000000)
        go (i + 1) s'
  go 0 12345
  return a

swap :: STUArray s Int Int -> Int -> Int -> ST s ()
swap a i j = do
  x <- readArray a i
  y <- readArray a j
  writeArray a i y
  writeArray a j x

partition :: STUArray s Int Int -> Int -> Int -> ST s Int
partition a lo hi = do
  p <- readArray a hi
  let loop i j
        | j >= hi = swap a i hi >> return i
        | otherwise = do
            x <- readArray a j
            if x < p then swap a i j >> loop (i + 1) (j + 1) else loop i (j + 1)
  loop lo lo

qsort :: STUArray s Int Int -> Int -> Int -> ST s ()
qsort a lo hi = when (lo < hi) $ do
  m <- partition a lo hi
  qsort a lo (m - 1)
  qsort a (m + 1) hi

check :: UArray Int Int -> Int -> Int -> Int -> Bool -> (Int, Bool)
check a n i acc ok
  | i >= n = (acc, ok)
  | otherwise =
      let x = a ! i
          ok' = ok && (i == 0 || a ! (i - 1) <= x)
       in ok' `seq` check a n (i + 1) ((acc + x * (i `rem` 7)) `rem` 1000000007) ok'

main :: IO ()
main = do
  [s] <- getArgs
  let n = read s :: Int
      a = runSTUArray (do a <- build n; qsort a 0 (n - 1); return a)
  print (check a n 0 0 True)
