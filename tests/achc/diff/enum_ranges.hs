-- enumFromThenTo ranges on Int, Char, Integer and Double, including negative steps
module Main where

main :: IO ()
main = do
  print [1, 3 .. 10 :: Int]
  print [10, 8 .. 1 :: Int]
  print [5, 4 .. 1 :: Int]
  print [1 .. 0 :: Int]
  print (take 4 [3, 3 .. 3 :: Int])
  print [-1, -3 .. -10 :: Int]
  print [-5 .. 5 :: Int]
  print [0, 5 .. 20 :: Int]
  print [20, 15 .. 0 :: Int]
  print ['a', 'c' .. 'k']
  print ['z', 'x' .. 'q']
  print ['a' .. 'f']
  print (take 5 ['x' ..])
  print [0.1, 0.2 .. 1.0 :: Double]
  print (length [0.1, 0.2 .. 1.0 :: Double])
  print [1.0 .. 5.0 :: Double]
  print [1.5 .. 4 :: Double]
  print [10, 8.5 .. 1 :: Double]
  print [1.0, 0.5 .. -1.0 :: Double]
  print [2 ^ 64, 2 ^ 64 + 5 .. 2 ^ 64 + 20 :: Integer]
  print [100, 90 .. 0 :: Integer]
  print (take 5 [10, 20 ..] :: [Int])
  print (sum [1 .. 100 :: Int], sum [1, 3 .. 99 :: Int])
  print (enumFromThenTo 1 4 (20 :: Int))
  print (enumFromTo 'a' 'e', enumFromThen 1 (3 :: Int) !! 10)
