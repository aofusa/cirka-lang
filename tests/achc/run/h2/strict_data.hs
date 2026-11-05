{-# LANGUAGE StrictData #-}
-- StrictData: fields are strict unless marked lazy with `~` (FEAT-0112).
module Main where

data P = P Int ~Int

data R = R { rx :: Int, ry :: ~Int }

main :: IO ()
main = do
  case P 1 undefined of P a _ -> print a
  print (rx (R 2 undefined))
  case P undefined 3 of P _ b -> print b
