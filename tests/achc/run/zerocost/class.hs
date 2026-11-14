-- same-cost-as: class_hand.hs
-- A type class costs nothing (FEAT-0061): the overloaded loop, used at
-- `Int`, runs and compiles like class_hand.hs (no dictionary at run time).
module Main where

class Vec a where
  vzero :: a
  vadd :: a -> a -> a
  vscale :: Int -> a -> a

instance Vec Int where
  vzero = 0
  vadd = (+)
  vscale k x = k * x

sumScaled :: Vec a => a -> a -> Int -> a
sumScaled v acc 0 = acc
sumScaled v acc n = sumScaled v (vadd acc (vscale n v)) (n - 1)

main :: IO ()
main = print (sumScaled (3 :: Int) vzero 100000)
