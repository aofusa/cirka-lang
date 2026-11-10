{-# LANGUAGE BangPatterns #-}
-- opt-levels: 2
-- max-allocs: 100
-- max-forced: 100
-- A loop whose lazily passed arguments are cheap and safe to evaluate
-- (FEAT-0078): once `fn` is unboxed they only read unboxed values, so they
-- count as values and `go` takes them unboxed. Before, every iteration
-- built thunks and boxes (about a million allocations here).
module Main where

import System.Environment (getArgs)

inside :: Double -> Double -> Bool
inside cr ci = go 0 0 0
  where
    go :: Int -> Double -> Double -> Bool
    go k zr zi
      | k >= 50 = True
      | tr + ti > 4 = False
      | otherwise = go (k + 1) (tr - ti + cr) (2 * zr * zi + ci)
      where
        tr = zr * zr
        ti = zi * zi

count :: Int -> Int
count n = rows 0 0
  where
    fn = fromIntegral n :: Double
    rows y !acc
      | y >= n = acc
      | otherwise = rows (y + 1) (cols (2 * fromIntegral y / fn - 1) 0 acc)
    cols ci x !acc
      | x >= n = acc
      | inside (2 * fromIntegral x / fn - 1.5) ci = cols ci (x + 1) (acc + 1)
      | otherwise = cols ci (x + 1) acc

main :: IO ()
main = do
  args <- getArgs
  print (count (case args of { (s : _) -> read s; [] -> 100 }))
