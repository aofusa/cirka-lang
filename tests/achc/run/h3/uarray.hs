{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- max-array-copies: 1
-- Unboxed unique arrays (FEAT-0123): Int / Double / Char / Bool elements,
-- in-place writes, strict elements and freeze / thaw. The only copy is the
-- `thaw` of the array `cs` that is used again afterwards.
module Main where

import Cirka.Ownership
import Cirka.Array.Unboxed (UArray, (!))
import qualified Cirka.Array.Unboxed as U

-- Prefix sums in place.
prefix :: Unique (UArray Int) %1 -> Unique (UArray Int)
prefix u0 = case U.length u0 of
  (u1, Ur n) -> go u1 1
    where
      go u i
        | i >= n = u
        | otherwise = case U.read u (i - 1) of
            (u', Ur p) -> go (U.modify u' i (+ p)) (i + 1)

main :: IO ()
main = do
  let xs = freeze (prefix (U.fromListU [1 .. 10 :: Int]))
  print (U.toList xs)
  print (U.size xs, xs ! 9)
  let ds = U.generate 4 (\i -> fromIntegral i / 2 :: Double)
  print (U.toList ds)
  let cs = U.fromList "abc"
      cs' = freeze (U.swap (U.thaw cs) 0 2)
  -- `cs` is still used after the thaw, so the thaw copies it.
  print (U.toList cs', U.toList cs)
  print (U.toList (freeze (U.write (U.newArray 3 False) 1 True)))
  print (U.toList (U.fromList ([] :: [Int])))
  print (minBound :: Int, maxBound :: Int) >> print (U.toList (U.fromList [minBound, maxBound :: Int]))
