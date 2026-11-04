-- Fibonacci numbers: naive recursion, iteration and a lazy list.
fib :: Int -> Int
fib n
  | n < 2 = n
  | otherwise = fib (n - 1) + fib (n - 2)

fibIter :: Int -> Int
fibIter n = go n 0 1
  where
    go 0 a _ = a
    go k a b = go (k - 1) b (a + b)

fibs :: [Int]
fibs = 0 : 1 : zipWith (+) fibs (tail fibs)

main :: IO ()
main = do
  print (fib 20)
  print (fibIter 90)
  print (fibs !! 50)
  print (sum (takeWhile (< 4000000) (filter even fibs)))
