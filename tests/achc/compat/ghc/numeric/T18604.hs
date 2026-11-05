-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T18604.hs (BSD-3-Clause; see LICENSE in this directory).
module Main (main) where

import Numeric.Natural

main :: IO ()
main = print (n - s)
  where
    n, s :: Natural
    n = 137503105969312982142385040956303729937425409769904987267247644890331944583201
    s = 370814112419299627365008243601943822482
