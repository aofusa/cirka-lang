-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/arith006.hs (BSD-3-Clause; see LICENSE in this directory).
-- !!! printing Floats; was a bug in hbc (reported by andy)
--

main = print ((fromIntegral (42 :: Int)) :: Float)
