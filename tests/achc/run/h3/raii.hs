{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
-- ghc: skip
-- RAII with Drop / Release (FEAT-0122): `release` is inserted at the end of
-- the IO block, in reverse binding order; `drop` where a value is left
-- unused (Unique has an implicit Drop).
module Main where

import Cirka.Ownership

type Res :: LType
data Res = Res (Ur String)

instance Release Res where
  release (Res (Ur n)) = putStrLn ("release " ++ n)

acquire :: String -> IO Res
acquire n = do
  putStrLn ("acquire " ++ n)
  returnL (Res (Ur n))

-- Uses a resource and gives it back.
use :: Res %1 -> IO Res
use (Res (Ur n)) = do
  putStrLn ("use " ++ n)
  returnL (Res (Ur n))

-- Consumes it explicitly (no release is inserted for it).
closeNow :: Res %1 -> IO ()
closeNow r = release r

-- A unique value left unused on one branch is dropped there.
pick :: Bool -> Unique Int %1 -> Int
pick b u = if b then freeze u else 0

block :: IO ()
block = do
  a <- acquire "a"
  b <- acquire "b"
  c <- acquire "c"
  b' <- use b
  closeNow c
  putStrLn "end of block"

main :: IO ()
main = do
  block
  print (pick True (unique 7) + pick False (unique 8))
  r <- acquire "main"
  putStrLn "done"
