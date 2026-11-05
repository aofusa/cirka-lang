{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
-- Linearity inference (FEAT-0128): variables whose values are copied or
-- dropped stay `Type`, so instantiating them with a linear-only type is
-- still E0310.
module Main where

type Res :: LType
data Res = Res Int

open :: Int -> Res
open n = Res n

close :: Res %1 -> Int
close (Res n) = n

dupl x = (x, x)

first (x, _) = x

closeBoth :: (Res, Res) %1 -> Int
closeBoth (a, b) = close a + close b

copied :: Int -> Int
copied n = closeBoth (dupl (open n))

dropped :: Int -> Int
dropped n = close (first (open n, open 1))

main :: IO ()
main = return ()
