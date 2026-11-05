-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T5628.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE EmptyDataDecls, StandaloneDeriving #-}
module Main where

data Z
deriving instance Eq Z

g :: Z
g = g

main :: IO ()
main = print (g == g)
