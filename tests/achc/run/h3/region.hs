{-# LANGUAGE CirkaRegions #-}
-- ghc: skip
-- Regions (FEAT-0127): values that refer to each other through region
-- references (a ring of nodes, a knot) are freed with the region, without
-- the cycle collector and without leaks.
module Main where

import Cirka.Region

data Node r = Node Int (RefIn r (Node r))

-- A ring of n nodes: node i points to node (i + 1) mod n.
ring :: Arena r -> Int -> RIO r (RefIn r (Node r))
ring ar n = do
  first <- newRefIn ar (Node 0 undefined)
  let build i prev
        | i == n = writeRefIn prev (Node (n - 1) first)
        | otherwise = do
            next <- newRefIn ar (Node i undefined)
            writeRefIn prev (Node (i - 1) next)
            build (i + 1) next
  build 1 first
  return first

-- Follows the ring `k` steps, summing the labels.
walk :: RefIn r (Node r) -> Int -> Int -> RIO r Int
walk _ 0 acc = return acc
walk ref k acc = do
  Node v next <- readRefIn ref
  walk next (k - 1) (acc + v)

main :: IO ()
main = do
  s <- withRegion (\ar -> do
    r <- ring ar 1000
    walk r 2500 0)
  print s
  -- The syntax: `region r in e` is `withRegion (\r -> e)`.
  t <- region ar in do
    x <- allocIn ar [1, 2, 3 :: Int]
    c <- newRefIn ar (0 :: Int)
    xs <- readIn x
    mapM_ (\v -> modifyRefIn c (+ v)) xs
    liftIO (putStrLn "inside the region")
    readRefIn c
  print t
