{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- max-array-copies: 1
-- Unique arrays (FEAT-0123): in-place writes, reads that keep the array
-- unique, freeze / thaw, and the immutable operations. The only copy is the
-- `thaw` of the shared array `a`.
module Main where

import Cirka.Ownership
import Cirka.Array (Array, (!))
import qualified Cirka.Array as A

-- Fills a[i] = i * i in place.
squares :: Int -> Unique (Array Int)
squares n = go (A.newArray n 0) 0
  where
    go u i
      | i >= n = u
      | otherwise = go (A.write u i (i * i)) (i + 1)

-- Sum of the elements, read through the unique array.
total :: Unique (Array Int) %1 -> (Unique (Array Int), Ur Int)
total u0 = case A.length u0 of
  (u1, Ur n) -> go u1 0 0
    where
      go u i acc
        | i >= n = (u, Ur acc)
        | otherwise = case A.read u i of
            (u', Ur x) -> go u' (i + 1) (acc + x)

-- Reverses in place.
reverseU :: Unique (Array e) %1 -> Unique (Array e)
reverseU u0 = case A.length u0 of
  (u1, Ur n) -> go u1 0 (n - 1)
    where
      go u i j
        | i >= j = u
        | otherwise = go (A.swap u i j) (i + 1) (j - 1)

main :: IO ()
main = do
  case total (squares 10) of
    (u, Ur s) -> do
      print s
      print (A.toList (freeze u))
  let a = A.fromList "hello"
  -- thaw copies a shared array: `a` is unchanged.
  let b = freeze (reverseU (A.thaw a))
  putStrLn (A.toList a)
  putStrLn (A.toList b)
  print (A.size a, a ! 1, b ! 1)
  let g = A.generate 5 (\i -> i * 10)
  print (A.toList (A.accum (+) g [(0, 1), (4, 2), (0, 3)]))
  print (A.toList (freeze (A.modify (A.fromListU [1, 2, 3 :: Int]) 2 negate)))
  -- elements stay lazy until read
  let l = freeze (A.write (A.newArray 3 (undefined :: Int)) 1 7)
  print (l ! 1)
