-- binary-trees (Benchmarks Game) with ownership: the tree is a linear-only
-- value (`type Tree :: LType`), built once and consumed by `check`, so
-- every node is freed as soon as it is counted (no reference count ever
-- exceeds 1). Same output as rust/main.rs.
module Main where

import System.Environment (getArgs)

type Tree :: LType
data Tree = Leaf | Node Tree Tree

make :: Int -> Tree
make 0 = Node Leaf Leaf
make d = Node (make (d - 1)) (make (d - 1))

-- Consumes the tree.
check :: Tree %1 -> Int
check Leaf = 0
check (Node l r) = 1 + check l + check r

sumTrees :: Int -> Int -> Int -> Int
sumTrees 0 _ !acc = acc
sumTrees n d !acc = sumTrees (n - 1) d (acc + check (make d))

depths :: Int -> Int -> IO ()
depths d maxd
  | d > maxd = return ()
  | otherwise = do
      let n = 2 ^ (maxd - d + 4) :: Int
      putStrLn (show n ++ "\t trees of depth " ++ show d ++ "\t check: " ++ show (sumTrees n d 0))
      depths (d + 2) maxd

main :: IO ()
main = do
  args <- getArgs
  let maxd = max 6 (case args of { (a : _) -> read a; [] -> 10 })
  putStrLn ("stretch tree of depth " ++ show (maxd + 1) ++ "\t check: " ++ show (check (make (maxd + 1))))
  -- The long-lived tree is created now and consumed at the end.
  let long = make maxd
  depths 4 maxd
  putStrLn ("long lived tree of depth " ++ show maxd ++ "\t check: " ++ show (check long))
