-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun048.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash, UnboxedTuples #-}

module Main where

import GHC.Exts (Int#, Double#)

main :: IO ()
main = let f = int2Integer# 0# in putStrLn ""


{-# NOINLINE int2Integer# #-}
int2Integer# :: Int# -> (# Int#, Double# #)
int2Integer# x = (# x, 1.0## #)
