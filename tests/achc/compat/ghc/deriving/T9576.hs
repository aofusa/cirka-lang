-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T9576.hs (BSD-3-Clause; see LICENSE in this directory).
{-# OPTIONS_GHC -fdefer-type-errors #-}

module Main where

data Foo = MkFoo
data Bar = MkBar Foo deriving Show

main = do { print True; print (MkBar MkFoo) }

