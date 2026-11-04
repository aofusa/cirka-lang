-- Factorials with explicit recursion, an accumulator and a fold.
fact :: Int -> Int
fact 0 = 1
fact n = n * fact (n - 1)

factAcc :: Int -> Int
factAcc n = go n 1
  where
    go 0 acc = acc
    go k acc = go (k - 1) (acc * k)

main :: IO ()
main = do
  print (fact 10)
  print (factAcc 20)
  print (product [1 .. 15])
  mapM_ (print . fact) [0 .. 5]
