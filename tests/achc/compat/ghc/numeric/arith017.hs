-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/arith017.hs (BSD-3-Clause; see LICENSE in this directory).
-- !!! test for a bug in integer->{Int,Word}64 conversion in GHC 5.04.x

import Data.Int
import Data.Word

main = do
  print (fromIntegral ((2^30 -1 + 2^30) - (2^30 + 2^30 :: Integer))
                :: Data.Int.Int64)
  print (fromIntegral ((2^30 -1 + 2^30) - (2^30 + 2^30 :: Integer))
                :: Data.Word.Word64)
