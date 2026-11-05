-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun004.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

data Hash = Hash{ (#) :: Int }
 deriving (Show, Read)

main =
  do print s
     print (read s :: Hash)
 where
  s = show (Hash 3)
