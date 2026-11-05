-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/numrun016.hs (BSD-3-Clause; see LICENSE in this directory).
-- test for trac #8532

import Data.Complex

main :: IO ()
main = do
    print $ acosh ((-1)::Complex Double)
    print $ acosh ((-1)::Complex Float)
