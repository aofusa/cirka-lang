{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- max-array-copies: 0
-- BUG-0009: a linear-only tuple component is evaluated before the tuple is
-- built (Strict-Linear). As a thunk it held a second reference to the
-- array, so the write inside it copied the array.
module Main where

import Cirka.Ownership
import qualified Cirka.Array as A

-- Swaps adjacent elements and returns the array in a tuple.
step :: Unique (A.Array Int) %1 -> Int -> (Unique (A.Array Int), Ur Int)
step u i = (A.swap u i (i + 1), Ur i)

walk :: Unique (A.Array Int) %1 -> Int -> Int -> Unique (A.Array Int)
walk u i n
  | i >= n = u
  | otherwise = case step u i of
      (u', Ur _) -> walk u' (i + 1) n

main :: IO ()
main = print (A.toList (freeze (walk (A.fromListU [1 .. 6 :: Int]) 0 5)))
