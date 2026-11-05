-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun015.hs (BSD-3-Clause; see LICENSE in this directory).
-- The leading underscore killed GHC 5.04

module Main where

data Obj = Obj {_id, p1, p2::Int} deriving (Show, Read)


main = print (show (read "Obj {_id=1,p1=10,p2=20}" :: Obj))
