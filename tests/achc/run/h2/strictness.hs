-- Strictness analysis (FEAT-0112): the accumulators below create no
-- thunks. `sum [1 .. n]` still allocates a cons cell, a box and a lazy
-- tail per element (no list fusion yet); the loops allocate nothing.
-- opt-levels: 2
-- max-allocs: 3001000
-- max-forced: 1001000
{-# LANGUAGE BangPatterns #-}
module Main where

-- A `where` loop capturing `n` (lambda lifted, then unboxed).
sumTo :: Int -> Int
sumTo n = go 0 1
  where
    go acc i = if i > n then acc else go (acc + i) (i + 1)

-- A local overloaded loop (no signature: generalized, then specialized)
-- with two accumulators. They end up in a lazy pair, so only the bang
-- patterns make them strict (GHC behaves the same).
stats :: Int -> (Int, Int)
stats n = loop 0 0 n
  where
    loop !s !q k
      | k == 0 = (s, q)
      | otherwise = loop (s + k) (q + k * k) (k - 1)

main :: IO ()
main = do
  print (sum [1 .. 1000000 :: Int])
  print (sumTo 1000000)
  print (stats 1000000)
  let count :: Int -> Int -> Int
      count acc k = if k == 0 then acc else count (acc + 2) (k - 1)
  print (count 0 1000000)
