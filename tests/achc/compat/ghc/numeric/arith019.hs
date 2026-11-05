-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/arith019.hs (BSD-3-Clause; see LICENSE in this directory).
-- demonstrates a bug in mulIntMayOflo in GHC 6.5 on 64-bit arches
-- (trac #867).
-- It thought it could represent 3049800625 * 3049800625 in an I#.

i :: Integer
i = 3049800625

main :: IO ()
main = print (i * i)

