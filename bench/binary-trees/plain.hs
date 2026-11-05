-- binary-trees (Benchmarks Game), single-threaded plain Haskell: aCHC and
-- the GHC -O2 baseline. Same output as rust/main.rs.
module Main where

import System.Environment (getArgs)

data Tree = Leaf | Node Tree Tree

make :: Int -> Tree
make 0 = Node Leaf Leaf
make d = Node (make (d - 1)) (make (d - 1))

check :: Tree -> Int
check Leaf = 0
check (Node l r) = 1 + check l + check r

-- Sum of `check` over `n` fresh trees of depth `d`.
sumTrees :: Int -> Int -> Int -> Int
sumTrees 0 _ acc = acc
sumTrees n d acc = let acc' = acc + check (make d) in acc' `seq` sumTrees (n - 1) d acc'

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
  let long = make maxd
  depths 4 maxd
  putStrLn ("long lived tree of depth " ++ show maxd ++ "\t check: " ++ show (check long))
