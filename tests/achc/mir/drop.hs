{-# LANGUAGE LinearTypes #-}
-- Weaken-Drop (FEAT-0122): a unique value used on one branch only is
-- dropped at the start of the other (a call of `dropThen`).
module Main where

import Cirka.Ownership

pick :: Bool -> Unique Int %1 -> Int
pick b u = if b then freeze u else 0

main :: IO ()
main = print (pick False (unique 1))
