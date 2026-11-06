{-# LANGUAGE BangPatterns #-}
-- opt-levels: 2
-- max-allocs: 2000
-- A loop calling a function argument it passes along unchanged
-- (FEAT-0061): each call site gets a copy of the loop with the function
-- (here a lambda capturing `k`) applied in place, so the `Int` argument and
-- the `Double` result are not boxed (2 allocations per iteration before).
module Main where

import System.Environment (getArgs)

sumWith :: (Int -> Double) -> Int -> Double
sumWith f n = go 0 0
  where
    go i !s
      | i >= n = s
      | otherwise = go (i + 1) (s + f i)

main :: IO ()
main = do
  args <- getArgs
  let k = fromIntegral (length args + 2) :: Double
  print (sumWith (\i -> fromIntegral i * 0.5) 1000000)
  print (sumWith (\i -> fromIntegral i / k) 1000000)
