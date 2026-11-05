-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun024.hs (BSD-3-Clause; see LICENSE in this directory).
-- !!! test super-dictionary grabification
--

main = putStr (show (is_one (1.2::Double)))

is_one :: RealFloat a => a -> Bool

is_one x = x == 1.0
