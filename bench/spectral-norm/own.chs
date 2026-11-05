-- spectral-norm (Benchmarks Game) with unboxed arrays: each product builds
-- a fresh `UArray Double` (written in place while it is still unique) and
-- reads the input by index, as rust/main.rs does with slices. Same output
-- as rust/main.rs.
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as U
import System.Environment (getArgs)

type Vec = U.UArray Double

a :: Int -> Int -> Double
a i j = 1 / fromIntegral ((i + j) * (i + j + 1) `quot` 2 + i + 1)

-- Row i of the product: sum over j of f i j * u[j].
mul :: (Int -> Int -> Double) -> Int -> Vec -> Vec
mul f n u = U.generate n row
  where
    row i = go i 0 0
    go i j !s
      | j >= n = s
      | otherwise = go i (j + 1) (s + f i j * (u U.! j))

mulAtAv :: Int -> Vec -> Vec
mulAtAv n u = mul (\i j -> a j i) n (mul a n u)

power :: Int -> Int -> Vec -> (Vec, Vec)
power n k u =
  let v = mulAtAv n u
      u' = mulAtAv n v
   in if k <= 1 then (u', v) else power n (k - 1) u'

dot :: Int -> Vec -> Vec -> Double
dot n x y = go 0 0
  where
    go i !s
      | i >= n = s
      | otherwise = go (i + 1) (s + (x U.! i) * (y U.! i))

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
      (u, v) = power n 10 (U.generate n (\_ -> 1))
  putStrLn (fixed9 (sqrt (dot n u v / dot n v v)))
