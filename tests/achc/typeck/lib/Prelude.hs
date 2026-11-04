{-# LANGUAGE NoImplicitPrelude #-}
module Prelude (module Prelude, module Cirka.Prim) where

import Cirka.Prim

infixr 9 .
infixl 6 +, -
infix 4 ==
infixr 5 ++
infixl 1 >>=, >>
infixr 0 $

(+), (-) :: Int -> Int -> Int
(+) = primIntAdd
(-) = primIntSub

(==) :: Int -> Int -> Bool
(==) = primIntEq

negate :: Int -> Int
negate x = 0 - x

(.) :: (b -> c) -> (a -> b) -> a -> c
(f . g) x = f (g x)

($) :: (a -> b) -> a -> b
f $ x = f x

(>>=) :: IO a -> (a -> IO b) -> IO b
(>>=) = primBindIO

(>>) :: IO a -> IO b -> IO b
m >> k = m >>= \_ -> k

return :: a -> IO a
return = primReturnIO

data Maybe a = Nothing | Just a

type String = [Char]

map :: (a -> b) -> [a] -> [b]
map _ [] = []
map f (x:xs) = f x : map f xs

(++) :: [a] -> [a] -> [a]
[] ++ ys = ys
(x:xs) ++ ys = x : xs ++ ys

concatMap :: (a -> [b]) -> [a] -> [b]
concatMap _ [] = []
concatMap f (x:xs) = f x ++ concatMap f xs

foldr :: (a -> b -> b) -> b -> [a] -> b
foldr _ z [] = z
foldr f z (x:xs) = f x (foldr f z xs)

enumFromTo :: Int -> Int -> [Int]
enumFromTo a b = if a == b then [b] else a : enumFromTo (a + 1) b

otherwise :: Bool
otherwise = True
