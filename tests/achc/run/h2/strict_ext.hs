{-# LANGUAGE BangPatterns #-}
-- Bang patterns in `let` and `where`, `seq` and `$!` (FEAT-0112).
module Main where

data Lazy = Lazy Int Int
data Strict = Strict !Int Int

firstL :: Lazy -> Int
firstL (Lazy a _) = a

firstS :: Strict -> Int
firstS (Strict a _) = a

mean :: [Double] -> Double
mean = go 0 0
  where
    go :: Double -> Int -> [Double] -> Double
    go !s !n [] = s / fromIntegral n
    go !s !n (x:xs) = go (s + x) (n + 1) xs

main :: IO ()
main = do
  print (firstL (Lazy 1 undefined))
  print (firstS (Strict 2 undefined))
  let !x = length [1, 2, 3 :: Int]
      y = undefined :: Int
  print x
  print (mean [1, 2, 3, 4])
  print (const 1 (undefined :: Int) :: Int)
  print (((\_ -> 5) $! (3 :: Int)) :: Int)
  print (let z = undefined :: Int in z `seq` (6 :: Int))
