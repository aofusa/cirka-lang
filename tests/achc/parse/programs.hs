module Main (main) where

import Data.List (sortBy, foldl')
import qualified Data.Map as M

infixl 6 |+|

data Vec = Vec { vx :: !Double, vy :: !Double } deriving (Eq, Show)

(|+|) :: Vec -> Vec -> Vec
Vec a b |+| Vec c d = Vec (a + c) (b + d)

data Tree a = Leaf | Node (Tree a) a (Tree a)

insert :: Ord a => a -> Tree a -> Tree a
insert x Leaf = Node Leaf x Leaf
insert x t@(Node l y r)
  | x < y = Node (insert x l) y r
  | x > y = Node l y (insert x r)
  | otherwise = t

toList :: Tree a -> [a]
toList Leaf = []
toList (Node l x r) = toList l ++ [x] ++ toList r

primes :: [Int]
primes = sieve [2 ..] where sieve (p:xs) = p : sieve [x | x <- xs, x `mod` p /= 0]

queens :: Int -> [[Int]]
queens n = go n
  where
    go 0 = [[]]
    go k = [q : qs | qs <- go (k - 1), q <- [1 .. n], safe q qs]
    safe q qs = and [q /= c && abs (q - c) /= d | (d, c) <- zip [1 ..] qs]

class Shape a where
  area :: a -> Double
  name :: a -> String
  name _ = "shape"

instance Shape Vec where
  area (Vec x y) = x * y

main :: IO ()
main = do
  print (take 10 primes)
  print (length (queens 6))
  let t = foldr insert Leaf [5, 3, 8, 1 :: Int]
  mapM_ (putStrLn . show) (toList t)
  print $ area (Vec 2 3 |+| Vec 1 1)
  print (negate 3, - 4, (subtract 1) 5, (`div` 2) 9, (2 ^) 10)
