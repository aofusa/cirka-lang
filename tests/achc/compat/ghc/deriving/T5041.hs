-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T5041.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}

module Main where

data T = T1# | T2# Int deriving( Read, Show )

foo :: [T]
foo = read "[ T1#, T2# 4, T2# 5 ]"
main = print foo

