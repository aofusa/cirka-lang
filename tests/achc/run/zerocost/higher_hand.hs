-- higher.hs with the functions written into the loops by hand.
module Main where

sumTo :: Int -> Int -> Int
sumTo acc 0 = acc
sumTo acc n = sumTo (acc + n) (n - 1)

sum3 :: Int -> Int -> Int
sum3 acc 0 = acc
sum3 acc n = sum3 (acc + 3 * n) (n - 1)

sum2 :: Int -> Int -> Int
sum2 acc 0 = acc
sum2 acc n = sum2 (acc + 2 * n) (n - 1)

main :: IO ()
main = do
  print (sumTo 0 100000)
  print (sum3 0 100000)
  print (sum2 0 100000)
