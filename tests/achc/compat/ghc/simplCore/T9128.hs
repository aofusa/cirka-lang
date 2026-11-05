-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T9128.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

newtype T a = MkT a

-- #9128: we treated x as absent!!!!

f x = let {-# NOINLINE h #-}
          h = case x of MkT g -> g
      in 
      h (h (h (h (h (h True)))))

main = print (f (MkT id))
