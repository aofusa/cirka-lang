-- same-cost-as: higher_hand.hs
-- A higher-order function costs nothing (FEAT-0061) when it is called
-- with a known function (an operator, a lambda, a partial application):
-- the loops run and compile like the first-order loops of higher_hand.hs.
module Main where

foldLoop :: (Int -> Int -> Int) -> Int -> Int -> Int
foldLoop f acc 0 = acc
foldLoop f acc n = foldLoop f (f acc n) (n - 1)

addScaled :: Int -> Int -> Int -> Int
addScaled k a i = a + k * i

main :: IO ()
main = do
  print (foldLoop (+) 0 100000)
  print (foldLoop (\a i -> a + 3 * i) 0 100000)
  print (foldLoop (addScaled 2) 0 100000)
