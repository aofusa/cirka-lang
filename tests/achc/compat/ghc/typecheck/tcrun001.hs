-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun001.hs (BSD-3-Clause; see LICENSE in this directory).
-- !! Test for (->) instances

module Main where

class Flob k where
  twice :: k a a  -> k a a

instance Flob (->) where
  twice f = f . f

inc :: Int -> Int
inc x = x+1

main = print (twice inc 2)


