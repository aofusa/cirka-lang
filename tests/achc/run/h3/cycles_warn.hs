-- expect-leak: 1
-- Cyclic data (FEAT-0126): under the default `--cycles=warn`, a recursive
-- value binding is accepted with W0401 and its cycle leaks (safely).
module Main where

main :: IO ()
main = do
  let xs = 1 : 2 : xs :: [Int]
  print (take 5 xs)
