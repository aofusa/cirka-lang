{-# LANGUAGE Strict #-}
-- Strict: arguments, `let` bindings and case alternatives are strict
-- unless marked lazy with `~` (FEAT-0112).
module Main where

lazyArg :: Int -> Int -> Int
lazyArg x ~_ = x

strictArg :: Int -> Int -> Int
strictArg x _ = x

main :: IO ()
main = do
  print (lazyArg 1 undefined)
  let ~u = undefined :: Int
  print (lazyArg 2 u)
  print (strictArg 3 undefined)
