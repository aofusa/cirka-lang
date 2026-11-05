-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun014.hs (BSD-3-Clause; see LICENSE in this directory).
-- !! cg014: main = -42 -- twice: in Float and Double

main = print ((show ( (-42) :: Float )) ++ " " ++ (show ( (-42) :: Double )) ++ "\n")
