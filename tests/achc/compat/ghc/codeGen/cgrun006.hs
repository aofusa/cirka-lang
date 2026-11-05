-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun006.hs (BSD-3-Clause; see LICENSE in this directory).
main = print (length thirteen_ones)
     where
        thirteen_ones   = take (13::Int) ones

        ones :: [Int]
        ones            = 1 : ones
