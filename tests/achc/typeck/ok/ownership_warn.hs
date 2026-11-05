{-# LANGUAGE LinearTypes #-}
-- flags: --ownership=warn
-- Ownership levels (FEAT-0125): under `ownership = warn`, unconsumed linear
-- values in a `.hs` file are warnings (E0302) and the program type checks.
module Main where

drop1 :: a %1 -> ()
drop1 _ = ()

unused :: a %1 -> b %1 -> a
unused x y = x

r = unused 'a' (drop1 'b')
