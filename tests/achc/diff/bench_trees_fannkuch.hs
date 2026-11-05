-- binary-trees depth 10 node counting and fannkuch-redux n=7
module Main where

import Data.List (delete)

data Tree = Leaf | Node Tree Tree

make :: Int -> Tree
make 0 = Node Leaf Leaf
make d = Node (make (d - 1)) (make (d - 1))

check :: Tree -> Int
check Leaf = 0
check (Node l r) = 1 + check l + check r

perms :: [Int] -> [[Int]]
perms [] = [[]]
perms xs = [x : p | x <- xs, p <- perms (delete x xs)]

flips :: [Int] -> Int
flips xs = go xs 0
  where
    go (1 : _) n = n
    go ys@(h : _) n = go (reverse (take h ys) ++ drop h ys) (n + 1)
    go [] n = n

fannkuch :: Int -> (Int, Int)
fannkuch n = (checksum, maxFlips)
  where
    fs = map flips (perms [1 .. n])
    checksum = sum (zipWith (\i f -> if even i then f else negate f) [0 :: Int ..] fs)
    maxFlips = maximum fs

main :: IO ()
main = do
  mapM_ (\d -> putStrLn ("depth " ++ show d ++ " nodes " ++ show (check (make d)))) [4, 6, 8, 10]
  let total = sum [check (make 6) | _ <- [1 .. 20 :: Int]]
  putStrLn ("20 trees of depth 6: " ++ show total)
  let (cs, mf) = fannkuch 7
  putStrLn ("fannkuch checksum " ++ show cs)
  putStrLn ("Pfannkuchen(7) = " ++ show mf)
  print (fannkuch 4)
  print (fannkuch 5)
