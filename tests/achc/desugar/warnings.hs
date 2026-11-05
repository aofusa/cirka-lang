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

-- Exhaustive although the decision tree keeps an impossible failure path
-- (BUG-0004): no warning.
both :: Maybe a -> Maybe a -> Int
both Nothing _ = 0
both _ Nothing = 1
both (Just _) (Just _) = 2

-- A guard that may fail does not cover its equation.
guarded :: Bool -> Int
guarded b | b = 1
guarded False = 0

main :: IO ()
main = return ()
