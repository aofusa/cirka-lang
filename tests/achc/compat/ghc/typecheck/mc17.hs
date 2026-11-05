-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/mc17.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MonadComprehensions, ParallelListComp #-}

-- !!! Parallel list comprehensions

module Main where

f xs = [ (x,y) | x <- xs, x>3 | y <- xs ]

main = print (f [0..10])

