{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
-- Linearity inference (FEAT-0128): a function without a signature that uses
-- the values of a type variable exactly once is generalized over `LType`
-- for that variable, so it takes linear-only values without annotations.
module Main where

type Res :: LType
data Res = Res Int

open :: Int -> Res
open n = Res n

close :: Res %1 -> Int
close (Res n) = n

swap (x, y) = (y, x)

pick b x y = if b then (x, y) else (y, x)

closeBoth :: (Res, Res) %1 -> Int
closeBoth (a, b) = close a + close b

swapped :: Int -> Int
swapped n = closeBoth (swap (open n, open 1))

local :: Int -> Int
local n = closeBoth (rot (open n, open 1))
  where
    rot (x, y) = (y, x)

picked :: Int -> Int
picked n = closeBoth (pick True (open n) (open 2))

-- Still usable at ordinary types and as an ordinary function value.
plain :: [(Int, Bool)]
plain = map swap [(True, 1), (False, 2)]

main :: IO ()
main = return ()
