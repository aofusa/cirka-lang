-- Double arithmetic with show, and Rational via Data.Ratio
import Data.Ratio

mean :: [Double] -> Double
mean xs = sum xs / fromIntegral (length xs)

harmonic :: Int -> Rational
harmonic n = sum [1 % fromIntegral k | k <- [1 .. n]]

main :: IO ()
main = do
  print (1 / 4 :: Double)
  print (0.5 + 0.25 :: Double)
  print (10 / 4 * 3 :: Double)
  print (sqrt 16 :: Double, sqrt 2 * sqrt 2 > (1.99 :: Double))
  print (mean [1, 2, 3, 4])
  print (fromIntegral (7 :: Int) / 2 :: Double)
  print (1.0e-2 :: Double, 1.5e10 :: Double, 123456.789 :: Double)
  print (-2.5 :: Double, [1.5, -2.0 :: Double])
  print (3 % 4 + 1 % 4 :: Rational)
  print (numerator (6 % 8), denominator (6 % 8 :: Rational))
  print (harmonic 5)
  print (negate (3 % 4) :: Rational)
  print (fromRational (7 % 8) :: Double)
  print (truncate (22 % 7 :: Rational) :: Int, round (7 % 2 :: Rational) :: Int)
  print (toRational (0.75 :: Double))
  print (map (\x -> x * x) [1 % 2, 2 % 3 :: Rational])
