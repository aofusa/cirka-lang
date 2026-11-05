-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T18509.hs (BSD-3-Clause; see LICENSE in this directory).
import Numeric.Natural

main :: IO ()
main = do
   print $ (0xFFFFFFFF0 * 0xFFFFFFFF0 :: Natural)
   print $ (2 :: Natural) ^ (190 :: Int)
