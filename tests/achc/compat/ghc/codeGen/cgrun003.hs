-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun003.hs (BSD-3-Clause; see LICENSE in this directory).
main = print (id2 (id2 id2) (42::Int))
--      where
--      id2 = s k k

-- id2 x = s k k x

id2 = s k k

s x y z = x z (y z)

k x y = x
