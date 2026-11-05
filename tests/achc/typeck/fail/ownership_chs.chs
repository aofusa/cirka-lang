-- flags: --ownership=off
-- Ownership levels (FEAT-0125): `.chs` files are always checked strictly,
-- whatever the level of `.hs` files (E0302).
module Main where

drop1 :: a %1 -> ()
drop1 _ = ()

r = drop1 'b'
