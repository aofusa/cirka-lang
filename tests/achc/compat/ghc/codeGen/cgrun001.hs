-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun001.hs (BSD-3-Clause; see LICENSE in this directory).
-- !! cg001: main = -42 -- take 1

main = print ( f () )
     where
       f :: a -> Int
       f x = -42
