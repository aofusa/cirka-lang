{-# LANGUAGE LinearTypes #-}
-- LType values have no reference counting operations (FEAT-0121,
-- 04-memory-model.md §3): a unique value is used exactly once, so Perceus
-- inserts neither `dup` (no second use) nor `drop` (it is always consumed).
-- no-rc: pass total chain
module Main where

import Cirka.Ownership

-- Passes a unique value through.
pass :: Unique [Int] %1 -> Unique [Int]
pass u = u

-- Consumes it once.
total :: Unique [Int] %1 -> Int
total u = sum (freeze u)

-- Hands it to two functions in turn.
chain :: Unique [Int] %1 -> Int
chain u = total (pass u)

-- The shared counterpart: the list is used twice (a `dup`).
shared :: [Int] -> Int
shared xs = sum xs + length xs

main :: IO ()
main = print (chain (unique [1, 2, 3]) + shared [4, 5])
