-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun049.hs (BSD-3-Clause; see LICENSE in this directory).

{-# LANGUAGE UnboxedTuples #-}

module Main where

type T a = Int -> (# Int, Int #)

{-# NOINLINE f2 #-}
f2 :: T a -> T a
f2 t = \x -> case t x of _ -> (# 3,4 #) -- NB: wildcard has unboxed tuple type

main = print $ case f2 (\x -> (# x, x + 1 #)) 10 of (# y, z #) -> y + z