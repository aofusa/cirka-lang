-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T4528a.hs (BSD-3-Clause; see LICENSE in this directory).
-- Crashed older GHCs when loaded into GHCi

module Main where

data T a = A | B | C deriving( Enum, Show )

main = print [A ..]
