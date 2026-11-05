{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- Linearity inference (FEAT-0128): functions without signatures that use
-- the values of a type variable once take `Unique` values without
-- annotations, and still work at ordinary types.
module Main where

import Cirka.Ownership

swap (x, y) = (y, x)

pick b x y = if b then (x, y) else (y, x)

bump :: Unique Int %1 -> Unique Int
bump u = unique (freeze u + 1)

total :: (Unique Int, Unique Int) %1 -> Int
total (a, b) = freeze (bump a) * 10 + freeze b

main :: IO ()
main = do
  print (total (swap (unique 1, unique 2)))
  print (total (pick False (unique 3) (unique 4)))
  print (map swap [(1 :: Int, 'a'), (2, 'b')])
  print (pick True "x" "y")
