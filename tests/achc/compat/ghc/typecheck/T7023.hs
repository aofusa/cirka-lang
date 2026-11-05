-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T7023.hs (BSD-3-Clause; see LICENSE in this directory).
{-# OPTIONS_GHC -fdefer-type-errors #-}

module Main where

a = ["x", 1 :: Int]

main :: IO ()
main = putStrLn "Hello World"

