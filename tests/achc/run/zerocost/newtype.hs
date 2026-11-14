-- same-cost-as: newtype_hand.hs
-- A newtype costs nothing (FEAT-0061): the loop over `Meters` runs and
-- compiles like the loop over `Int` of newtype_hand.hs.
module Main where

newtype Meters = Meters Int

addM :: Meters -> Meters -> Meters
addM (Meters a) (Meters b) = Meters (a + b)

scale :: Int -> Meters -> Meters
scale k (Meters a) = Meters (k * a)

total :: Meters -> Int -> Meters
total acc 0 = acc
total acc n = total (addM acc (scale 3 (Meters n))) (n - 1)

main :: IO ()
main = do
  let Meters r = total (Meters 0) 100000
  print r
