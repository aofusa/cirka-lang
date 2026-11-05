-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/TcNullaryTC.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MultiParamTypeClasses #-}

module Main where

class R where
  f :: Int -> Int
  g :: a -> a

instance R where
  f = (+1)
  g = id

main = print (g (f 0))
