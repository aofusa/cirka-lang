module Main where

data Color = Red | Green | Blue

name :: Color -> String
name Red = "red"
name Green = "green"
name Blue = "blue"

zipW :: (a -> b -> c) -> [a] -> [b] -> [c]
zipW f (x:xs) (y:ys) = f x y : zipW f xs ys
zipW _ _ _ = []

digit :: Int -> Char
digit 0 = '0'
digit 1 = '1'
digit _ = '?'

clamp :: Int -> Int
clamp n
  | n == 0 = 0
  | otherwise = n + 1

main :: IO ()
main = putStr (name Red)
