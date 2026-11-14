-- class.hs written by hand at `Int`.
module Main where

sumScaled :: Int -> Int -> Int -> Int
sumScaled v acc 0 = acc
sumScaled v acc n = sumScaled v (acc + n * v) (n - 1)

main :: IO ()
main = print (sumScaled 3 0 100000)
