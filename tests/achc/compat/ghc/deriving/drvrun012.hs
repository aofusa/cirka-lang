-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun012.hs (BSD-3-Clause; see LICENSE in this directory).
-- Tests readings of record syntax

module Main where

data Foo = Foo { x :: Baz, y :: Maybe Int } deriving (Read,Show)

infix 0 :%%
data Baz = Int :%% Int deriving( Read,Show)


main = print (read "Foo { x = 1 :%% 2, y = Just 4 }" :: Foo)
