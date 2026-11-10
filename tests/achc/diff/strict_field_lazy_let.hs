-- opt-levels: 0 2
-- BUG-0014: a lazily bound constructor is built when it is demanded, not
-- when it is bound: only then its strict fields get evaluated.
module Main where

data S = S !Int !Int

first :: S -> Int
first (S a _) = a

zero :: Int
zero = length ""

main :: IO ()
main = do
  let n = zero
      s = S (10 `div` n) n
  putStrLn "hello"
  print (if n > 0 then first s else 0)
  let d = 10 `div` n
      t = S d 1
  print (if n > 0 then first t else 1)
  let bad = error "never forced" :: Int
      u = S bad 2
  print (if n > 0 then first u else 2)
