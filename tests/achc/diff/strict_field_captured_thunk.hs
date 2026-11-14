-- opt-levels: 0 2
-- BUG-0020: a constructor in a lazy position whose strict field gets a
-- parameter or a captured variable is built when it is demanded: the
-- variable may be a thunk that nothing has demanded yet.
module Main where

import System.Environment (getArgs)

data P = P !Int

data Q = Q ![Int]

g :: Int -> Int
g k
  | k > 10 = g (k - 1)
  | k == 0 = error "boom"
  | otherwise = k + 41

f :: P -> Int -> Int
f _ 0 = 0
f p n = case p of P x -> x + f p (n - 1)

sumQ :: Q -> Int -> Int
sumQ _ 0 = 0
sumQ q n = case q of Q xs -> sum xs + sumQ q (n - 1)

-- A parameter that callers pass unevaluated.
wrap :: Int -> Int -> Int
wrap t n = f (P t) n

main :: IO ()
main = do
  args <- getArgs
  let d = length args
      t = g d
      u = replicate (d + 2) (100 `div` d)
  print (f (P t) d)
  print (f (P t) (d * 2))
  print (sumQ (Q u) d)
  print (wrap (g d) d)
  print (wrap (g (d + 5)) 3)
