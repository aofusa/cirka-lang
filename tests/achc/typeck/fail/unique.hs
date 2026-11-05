{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
-- Uniqueness errors (FEAT-0121): E0301 (a linear-only value bound or
-- passed with multiplicity ω) and E0310 (a `Type` variable instantiated
-- with a linear-only type).
module Main where

type Res :: LType
data Res = Res Int

-- Holds a linear-only field: linear-only too.
data Box = Box Res

open :: Int -> Res
open n = Res n

close :: Res %1 -> Int
close (Res n) = n

dupl :: a -> (a, a)
dupl x = (x, x)

unrestrictedParam :: Res -> Int
unrestrictedParam r = close r

unrestrictedArg :: Int
unrestrictedArg = keep (open 1)
  where
    keep :: Res -> Int
    keep _ = 0

copied :: (Res, Res)
copied = dupl (open 2)

boxTwice :: Box %1 -> (Box, Box)
boxTwice b = (b, b)

leaked :: Int -> Int
leaked n = let r = open n in n

global :: Res
global = open 3

lambda :: Int
lambda = (\r -> close r) (open 4)

main :: IO ()
main = return ()
