-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/arith009.hs (BSD-3-Clause; see LICENSE in this directory).
-- a prefix minus precedence test

f :: Int -> Int -> Int -> Int
f x y z = - x * y ^ z

main = putStr (shows (f 5 2 3) "\n")
