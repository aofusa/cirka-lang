-- mandelbrot (Benchmarks Game algorithm), single-threaded plain Haskell:
-- aCHC and the GHC -O2 baseline. Prints the number of points of the n x n
-- grid in the set instead of the PBM bitmap (as rust/main.rs).
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
  print (count (case args of { (s : _) -> read s; [] -> 200 }))
