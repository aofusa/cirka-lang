-- Integer arithmetic beyond 64 bits
fact :: Integer -> Integer
fact n = product [1 .. n]

fibPair :: Int -> (Integer, Integer)
fibPair 0 = (0, 1)
fibPair n = let (a, b) = fibPair (n - 1) in (b, a + b)

fib :: Int -> Integer
fib = fst . fibPair

powMod :: Integer -> Integer -> Integer -> Integer
powMod _ 0 m = 1 `mod` m
powMod b e m
  | even e = let h = powMod b (e `div` 2) m in h * h `mod` m
  | otherwise = b * powMod b (e - 1) m `mod` m

digitSum :: Integer -> Int
digitSum = sum . map (\c -> fromEnum c - 48) . show

main :: IO ()
main = do
  print (fact 20)
  print (fact 25)
  print (fact 50)
  print (fib 90)
  print (fib 150)
  print (2 ^ 100 :: Integer)
  print (digitSum (fact 100))
  print (powMod 3 200 1000000007)
  print (negate (2 ^ 70) `div` 7 :: Integer, negate (2 ^ 70) `mod` 7 :: Integer)
  print (gcd (fact 20) (2 ^ 30) :: Integer)
  print (fromIntegral (2 ^ 62 :: Integer) * 4 :: Integer)
  print (maxBound :: Int, fromIntegral (maxBound :: Int) + 1 :: Integer)
  print (compare (2 ^ 65) (3 ^ 41 :: Integer))
