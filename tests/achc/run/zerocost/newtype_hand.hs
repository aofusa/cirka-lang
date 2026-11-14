-- newtype.hs written by hand over `Int`.
module Main where

total :: Int -> Int -> Int
total acc 0 = acc
total acc n = total (acc + 3 * n) (n - 1)

main :: IO ()
main = print (total 0 100000)
