-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T4381.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

main = do
  print (scaleFloat 30000 1)
  print (scaleFloat (maxBound :: Int) 1)
