{-# LANGUAGE CirkaOwnership #-}
-- flags: --ownership=off
-- Ownership levels (FEAT-0125): `CirkaOwnership` in a `.hs` file makes it a
-- Cirka-mode module, checked strictly (E0302).
module Main where

unused :: a %1 -> b %1 -> a
unused x y = x

r = unused 'a' 'b'
