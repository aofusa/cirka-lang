-- Integer, Word, Float, Double and Rational against GHC.
import Data.Ratio

fact :: Integer -> Integer
fact n = product [1 .. n]

fib :: Int -> Integer
fib n = go n 0 1
  where
    go 0 a _ = a
    go k a b = go (k - 1) b (a + b)

main :: IO ()
main = do
  print (product [1 .. 100] :: Integer)
  print (fact 30 `div` fact 28, fib 200)
  print (2 ^ 64, 2 ^ 63 - 1 :: Integer, negate (2 ^ 63) :: Integer)
  print (12345678901234567890123 * 98765432109876543210)
  print [(a `div` b, a `mod` b, a `quot` b, a `rem` b) | a <- [7, -7, 2 ^ 70 + 5, negate (3 ^ 50)], b <- [3, -3, 2 ^ 40 + 1 :: Integer]]
  print (divMod (10 ^ 30) (10 ^ 12 + 7 :: Integer), gcd (2 ^ 70) (6 ^ 30 :: Integer), lcm 12 (18 :: Integer))
  print (fromIntegral (maxBound :: Int) + 1 :: Integer, fromInteger (2 ^ 64 + 3) :: Int)
  print (compare (2 ^ 80) (2 ^ 79 :: Integer), abs (negate (10 ^ 20)) :: Integer, signum (negate (10 ^ 20)) :: Integer)
  print (read "123456789012345678901234567890" :: Integer, read "-42" :: Integer)
  print (fromIntegral (2 ^ 100 :: Integer) :: Double, truncate (1e20 :: Double) :: Integer)
  print (round (2.5 :: Double) :: Integer, round (3.5 :: Double) :: Integer, floor (-0.5 :: Double) :: Integer, ceiling (1e25 :: Double) :: Integer)
  print [1 / 3 :: Float, 0.1 + 0.2, 1e10, 1.0e-2, 123456.78, 3.4028235e38, 1 / 0]
  print [1 / 3 :: Double, 0.1 + 0.2, 1e22, 5.0e-324, 0.1, 1234567.0, 12345678.9]
  print (realToFrac (0.1 :: Float) :: Double, realToFrac (0.1 :: Double) :: Float)
  print (sqrt 2 :: Float, pi :: Float, exp 1 :: Float, sin 1 :: Double, atan2 1 (-1) :: Double)
  print (decodeFloat (1.5 :: Double), decodeFloat (0.1 :: Float), encodeFloat 3 (-1) :: Double)
  print (floatDigits (1 :: Float), floatRange (1 :: Double), significand (8 :: Double), exponent (8 :: Double))
  print (toRational (0.75 :: Double), toRational (0.1 :: Float), fromRational (1 % 3) :: Double)
  print (numerator (6 % 8 :: Rational), denominator (6 % 8 :: Rational), (1 % 3 + 1 % 6) :: Rational)
  print (properFraction (-3.75 :: Double) :: (Integer, Double), isNaN (0 / 0 :: Float), isInfinite (1 / 0 :: Double))
  print (maxBound :: Word, minBound :: Word, fromIntegral (negate 1 :: Int) :: Word, (2 :: Word) - 3)
  print (div (maxBound :: Word) 3, toInteger (maxBound :: Word), fromIntegral (2 ^ 64 - 1 :: Integer) :: Word)
  print (read "3.25" :: Float, read "18446744073709551615" :: Word, show (2 ^ 70 :: Integer))
  print (sum (map fromIntegral [1 .. 100 :: Int]) :: Integer, [10 ^ 18, 10 ^ 19 .. 5 * 10 ^ 19 :: Integer])
  print (1e30 :: Rational, 5e-324 :: Double, 2.5e-3 :: Rational, -1.5e20 :: Rational, 1e-40 :: Float)
  print (scaleFloat 30000 1 :: Double, scaleFloat (maxBound :: Int) (1 :: Double), scaleFloat minBound (1 :: Double), scaleFloat 3 (1.5 :: Float))
