-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun005.hs (BSD-3-Clause; see LICENSE in this directory).
-- !! answer: 65532

main = print foo

foo :: Int
foo = ((1 + 2 + 32767 - 4) * 6) --later? `div` 3
