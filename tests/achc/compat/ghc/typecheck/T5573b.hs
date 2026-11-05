-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T5573b.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash, UnboxedTuples, BangPatterns #-}
module Main where

import GHC.Exts

{-# NOINLINE foo #-} -- Make it harder to get right
foo :: Double# -> (# (# Double#, Double# #), Double# #)
foo x = (# (# x, x #), x #)

main :: IO ()
main = case foo 1.0## of
        (# (# x, y #), z #) -> print (D# x + D# y + D# z)
