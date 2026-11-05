-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun039.hs (BSD-3-Clause; see LICENSE in this directory).
-- !!! From a Rick Morgan bug report:
-- !!! Single-method class with a locally-polymorphic
-- !!! method.

module Main where

class Poly a where
   poly :: a -> b -> b

instance Poly [a] where
   poly [] y = y
   poly x  y = y

main = print ("hurrah" `poly` "Hello, world!\n")
