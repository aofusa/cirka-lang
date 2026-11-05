-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T14918.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}
module Main where

data T  a = MkT  { runT  :: a, (##)  :: a } deriving (Read, Show)
data T# a = MkT# { runT# :: a, (###) :: a } deriving (Read, Show)

t1, t2 :: T Int
t1 = MkT (-1) 1
t2 = read $ show t1

t1#, t2# :: T# Int
t1# = MkT# (-1) 1
t2# = read $ show t1#

main :: IO ()
main = do
  print t2
  print t2#
