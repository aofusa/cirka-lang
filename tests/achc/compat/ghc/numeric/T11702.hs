-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T11702.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

testfn :: Word -> IO ()
testfn wseq = do
  print $ wseq `mod` 1

main = do
  testfn 5
  print $ (5 :: Word) `mod` 1
