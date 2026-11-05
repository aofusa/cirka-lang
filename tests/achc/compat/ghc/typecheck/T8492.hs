-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T8492.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

x :: ((->) Int) Bool
x = (==0)

main = print $ x 0
