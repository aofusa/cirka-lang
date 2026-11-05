-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun010.hs (BSD-3-Clause; see LICENSE in this directory).
main = print a
  where
    a :: Int
    b :: Int
    (a, b) = (3 + 4, 5 + 6)
