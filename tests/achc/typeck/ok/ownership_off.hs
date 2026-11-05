{-# LANGUAGE LinearTypes #-}
-- flags: --ownership=off
-- Ownership levels (FEAT-0125): under `ownership = off`, unconsumed linear
-- values in a `.hs` file are not reported.
module Main where

drop1 :: a %1 -> ()
drop1 _ = ()

unused :: a %1 -> b %1 -> a
unused x y = x

r = unused 'a' (drop1 'b')
