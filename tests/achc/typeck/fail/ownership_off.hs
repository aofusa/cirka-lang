{-# LANGUAGE LinearTypes #-}
-- flags: --ownership=off
-- Ownership levels (FEAT-0125): `off` only silences leaks; using a linear
-- value twice is an error at every level (E0303). `unused` is not reported.
module Main where

dup :: a %1 -> (a, a)
dup x = (x, x)

unused :: a %1 -> b %1 -> a
unused x y = x

r = dup 'a'
