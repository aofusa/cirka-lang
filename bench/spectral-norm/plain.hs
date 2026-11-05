-- spectral-norm (Benchmarks Game), single-threaded plain Haskell with lists:
-- aCHC and the GHC -O2 baseline. Same output as rust/main.rs.
module Main where

import System.Environment (getArgs)

a :: Int -> Int -> Double
a i j = 1 / fromIntegral ((i + j) * (i + j + 1) `quot` 2 + i + 1)

-- Strict dot product of row `f k` with `u` (k = 0, 1, ...).
dot :: (Int -> Double) -> [Double] -> Double
dot f = go 0 0
  where
    go !_ !s [] = s
    go k !s (x : xs) = go (k + 1) (s + f k * x) xs

mulAv, mulAtv, mulAtAv :: Int -> [Double] -> [Double]
mulAv n u = forceList [dot (a i) u | i <- [0 .. n - 1]]
mulAtv n u = forceList [dot (\j -> a j i) u | i <- [0 .. n - 1]]
mulAtAv n u = mulAtv n (mulAv n u)

forceList :: [Double] -> [Double]
forceList xs = go xs `seq` xs
  where
    go [] = ()
    go (y : ys) = y `seq` go ys

power :: Int -> Int -> [Double] -> ([Double], [Double])
power _ 0 u = (u, u)
power n k u =
  let v = mulAtAv n u
      u' = mulAtAv n v
   in if k == 1 then (u', v) else power n (k - 1) u'

fixed9 :: Double -> String
fixed9 x
  | x < 0 = '-' : fixed9 (negate x)
  | otherwise =
      let n = truncate (x * 1000000000 + 0.5) :: Int
          f = show (n `rem` 1000000000)
       in show (n `quot` 1000000000) ++ "." ++ replicate (9 - length f) '0' ++ f

main :: IO ()
main = do
  args <- getArgs
  let n = case args of { (s : _) -> read s; [] -> 100 }
      (u, v) = power n 10 (replicate n 1)
      vbv = sum (zipWith (*) u v)
      vv = sum (map (\x -> x * x) v)
  putStrLn (fixed9 (sqrt (vbv / vv)))
