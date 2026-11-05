-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun050.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE UnboxedTuples #-}

module Main where

type T a = Int -> (# Int, Int #)

{-# NOINLINE g #-}
--g :: (# Int, Int #) -> (# Int, Int #)
g t =  case t of r -> (r :: (# Int, Int #)) 

{-# NOINLINE f #-}
f :: T a -> T a
f t = \x -> case t x of r -> r


main = print $ case f (\x -> g (# x, x + 1 #)) 10 of (# y, z #) -> y + z