module Main where

data S = S { x :: !Int, y :: Int }

data T = A { f :: Int } | B { g :: Int }

s :: S
s = S { y = 1 }

t :: T -> T
t v = v { f = 1, g = 2 }

main = putStr "c"
