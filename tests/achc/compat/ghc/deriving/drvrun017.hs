-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun017.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}

-- Test Show on unboxed types

module Main where
import GHC.Base

data Foo = MkFoo Int# Float# Int deriving( Show )

main = print (MkFoo 3# 4.3# 2)