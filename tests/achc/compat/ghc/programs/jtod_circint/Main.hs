-- Imported from GHC 9.4.7 testsuite/tests/programs/jtod_circint/Main.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where
import Signal
import Bit

main = putStr test

test = stest

type B =  Stream Bit

stest = take 80 (shows z "\n")
  where z = one :: B
