{-# LANGUAGE MultiParamTypeClasses, FlexibleInstances, FlexibleContexts #-}
-- FEAT-0118: multi-parameter classes, flexible instances and contexts.
module Main where

class Convert a b where
  convert :: a -> b
  convertAll :: [a] -> [b]
  convertAll = map convert

instance Convert Int String where
  convert = show

instance Convert Bool Int where
  convert b = if b then 1 else 0

instance Convert a b => Convert [a] [b] where
  convert = map convert

class Container f a where
  empty :: f a
  insert :: a -> f a -> f a
  toL :: f a -> [a]

instance Container [] Int where
  empty = []
  insert = (:)
  toL = id

fill :: Container f Int => [Int] -> f Int
fill = foldr insert empty

via :: (Convert a b, Convert b c) => (a -> b) -> a -> c
via f x = convert (f x)

twice x = (convert x, convert x)

class Pretty a where
  pretty :: a -> String

instance Pretty [Char] where
  pretty s = "<" ++ s ++ ">"

instance Pretty (Maybe Int) where
  pretty = maybe "none" show

-- A class without parameters.
class Config where
  verbose :: Bool

instance Config where
  verbose = True

showAll :: Show (f Int) => f Int -> String
showAll = show

main :: IO ()
main = do
  putStrLn (convert (42 :: Int))
  print (convert True + (1 :: Int))
  print (convertAll [True, False, True] :: [Int])
  print (convert [True, False] :: [Int])
  print (toL (fill [1, 2, 3] :: [Int]))
  putStrLn (via (convert :: Bool -> Int) True)
  print (twice False :: (Int, Int))
  putStrLn (pretty "hi")
  putStrLn (pretty (Just (3 :: Int)))
  putStrLn (showAll (Just 5))
  print verbose
