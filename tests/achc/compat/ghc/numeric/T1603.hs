-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T1603.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

main = print (syn [-1])

syn :: [Int] -> [Int]
syn (d:ds) = rem d 0x40000000 : syn ds
syn [] = []
