module Main where

data Color = Red | Green | Blue

isRed :: Color -> Bool
isRed Red = True
isRed Green = False

first :: [a] -> a
first (x:_) = x

dup :: Bool -> Int
dup _ = 1
dup True = 2

main :: IO ()
main = return ()
