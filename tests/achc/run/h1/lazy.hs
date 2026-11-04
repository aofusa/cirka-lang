-- Lazy evaluation: infinite lists, knot-tying and unevaluated errors.
primes :: [Int]
primes = sieve [2 ..]
  where sieve (p:xs) = p : sieve [x | x <- xs, x `mod` p /= 0]

collatz :: Int -> [Int]
collatz 1 = [1]
collatz n
  | even n = n : collatz (n `div` 2)
  | otherwise = n : collatz (3 * n + 1)

ones :: [Int]
ones = 1 : ones

main :: IO ()
main = do
  mapM_ print (take 10 primes)
  print (length (collatz 27))
  print (take 3 (map fst (zip [1 ..] (repeat undefined))) !! 2)
  print (sum (take 100 ones))
  print (fst (1, error "never evaluated"))
  print (length [undefined, undefined, undefined])
  print (head (dropWhile (< 1000) (iterate (* 2) 1)))
  print (const 42 (head []))
