-- Integer and Int arithmetic edge cases: big numbers, divMod/quotRem, wraparound
module Main where

data V = V Int Integer deriving Show

fact :: Integer -> Integer
fact n = product [1 .. n]

main :: IO ()
main = do
  print (2 ^ 100 :: Integer)
  print (fact 25)
  print (fact 30 `div` fact 28)
  print (product [1 .. 20 :: Int])
  print ((-7) `divMod` 2 :: (Int, Int), (-7) `quotRem` 2 :: (Int, Int))
  print (7 `divMod` (-2) :: (Int, Int), 7 `quotRem` (-2) :: (Int, Int))
  print ((-7) `divMod` (-2) :: (Int, Int), (-7) `quotRem` (-2) :: (Int, Int))
  print ((-2 ^ 70) `divMod` 12345 :: (Integer, Integer))
  print ((-2 ^ 70) `quotRem` 12345 :: (Integer, Integer))
  print ((2 ^ 70) `divMod` (-12345) :: (Integer, Integer))
  print (gcd (2 ^ 100) (6 ^ 50) == (2 ^ 50 :: Integer))
  print (gcd (fact 20) (fact 25 `div` 7))
  print (lcm 12 18 :: Integer, lcm (2 ^ 40) (3 ^ 20) :: Integer)
  print (gcd 0 5 :: Int, gcd (-12) 18 :: Int)
  print (minBound :: Int)
  print (maxBound :: Int)
  print (negate (minBound :: Int))
  print (abs (minBound :: Int))
  print ((maxBound :: Int) + 1)
  print ((minBound :: Int) - 1)
  print ((maxBound :: Int) * 2)
  print (2 ^ 63 :: Int, 2 ^ 62 :: Int)
  print (fromIntegral (2 ^ 64 + 5 :: Integer) :: Int)
  print (toInteger (maxBound :: Int) + 1)
  print (V (-1) (-2 ^ 70))
  print (Just (-5 :: Integer), [negate 3 :: Integer])
  print (showsPrec 7 (-5 :: Int) "", showsPrec 6 (-5 :: Int) "")
  print (signum (-9 :: Integer), signum (0 :: Int), abs (-2 ^ 65 :: Integer))
  print ((-5) `mod` 3 :: Int, (-5) `rem` 3 :: Int, 5 `mod` (-3) :: Int)
  print (2 ^ 10 `div` 3 :: Integer, 10 ^ 18 + 1 :: Integer)
  print (compare (2 ^ 65) (2 ^ 64 :: Integer), (2 ^ 65 :: Integer) == 2 * 2 ^ 64)
  print (length (show (fact 100)))
