-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun047.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE UnboxedTuples #-}

-- !!! Check that unboxed tuples can be function arguments
module Main where

data Ex = Ex (# Int,Int #)

{-# NOINLINE f #-} -- Make it harder to get right
f :: (# Int,Int #) -> Int
f x = error "urk"

{-# NOINLINE g #-} -- Make it harder to get right
g (Ex (# x,y #)) = x


main = print $ g (Ex (# 10, f (# 20, 30 #) #))