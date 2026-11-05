-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun018.hs (BSD-3-Clause; see LICENSE in this directory).

-- Test Show on unboxed types

module Main where

data Foo = Int `MkFoo` Int deriving( Read, Show )

main = do { print (MkFoo 4 5)
          ; print (read "3 `MkFoo` 5" :: Foo) }
