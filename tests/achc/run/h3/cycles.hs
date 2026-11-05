-- flags: --cycles=collect
-- Cyclic data (FEAT-0126): recursive value bindings (knot-tying) and a
-- reference that points to itself are freed by the cycle collector
-- (trial deletion) under `--cycles=collect`: no leaks, no W0401.
module Main where

import Data.IORef

newtype Node = Node (IORef [Node])

ones :: [Int]
ones = 1 : ones

main :: IO ()
main = do
  let xs = 1 : 2 : xs :: [Int]
  print (take 5 xs)
  print (take 3 ones)
  let n = length (takeWhile (< 10) (let ys = 0 : map (+ 1) ys in ys :: [Int]))
  print n
  let evens = 0 : map (+ 1) odds
      odds = map (+ 1) evens
  print (take 4 (evens :: [Int]))
  r <- newIORef []
  writeIORef r [Node r]
  xs' <- readIORef r
  print (length xs')
  print (sum (take 1000 (cycle' [1, 2, 3])))
  where
    cycle' l = let c = l ++ c in c :: [Int]
