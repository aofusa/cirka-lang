-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun062.hs (BSD-3-Clause; see LICENSE in this directory).

module Main where

newtype T = C { f :: String }

{-
hugs (Sept 2006) gives
"bc"
Program error: Prelude.undefined
hugs trac #48
-}

main = do print $ case C "abc" of
                  C { f = v } -> v
          print $ case undefined of
                  C {} -> True

