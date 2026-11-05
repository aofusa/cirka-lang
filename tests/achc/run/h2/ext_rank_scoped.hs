{-# LANGUAGE ScopedTypeVariables, RankNTypes #-}
-- FEAT-0118: scoped type variables and rank-N types (annotations required).
module Main where

pairUp :: forall a. Show a => [a] -> [String]
pairUp xs = map f xs
  where
    f :: a -> String
    f x = show x ++ "!"

-- Without an explicit forall the inner `a` is a new variable.
plain :: [b] -> Int
plain ys = length (g ys)
  where
    g :: [a] -> [a]
    g = reverse

pick :: forall a. Ord a => [a] -> Maybe a
pick [] = Nothing
pick (x : xs) = Just (foldr (\y (m :: a) -> max y m) x xs)

applyBoth :: (forall x. [x] -> Int) -> ([Int], String) -> (Int, Int)
applyBoth f (a, b) = (f a, f b)

applyTwice :: (forall x. [x] -> [x]) -> ([Int], String) -> ([Int], String)
applyTwice f (a, b) = (f a, f b)

newtype Nat = Nat (forall a. (a -> a) -> a -> a)

toInt :: Nat -> Int
toInt (Nat n) = n (+ 1) 0

twoN :: Nat
twoN = Nat (\s z -> s (s z))

useId :: (forall a. a -> a) -> (Int, Bool)
useId f = (f 1, f True)

main :: IO ()
main = do
  print (pairUp [1, 2, 3 :: Int])
  print (plain "abc")
  print (pick [3, 1, 4, 1, 5 :: Int])
  print (applyBoth length ([1, 2, 3], "hello"))
  print (applyTwice reverse ([1, 2, 3], "hello"))
  print (applyTwice (take 2) ([1, 2, 3], "hello"))
  print (toInt twoN)
  print (useId id)
  let g (x :: Int) = x + 1
  print (g 41)
