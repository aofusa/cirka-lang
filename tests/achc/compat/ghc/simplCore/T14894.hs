-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T14894.hs (BSD-3-Clause; see LICENSE in this directory).
{-# OPTIONS_GHC -g1 -O #-}
import System.Environment
summap :: (Int -> Int) -> (Int -> Int)
summap f n = f 10
{-# NOINLINE summap #-}

main = do
  n <- length `fmap` getArgs
  print $ summap (+ n) n
