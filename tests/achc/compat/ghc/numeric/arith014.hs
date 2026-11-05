-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/arith014.hs (BSD-3-Clause; see LICENSE in this directory).
-- Test behaviour of fromInteger when the target type is out of range.

main :: IO ()
main = 
  print [
    fromInteger maxInt2 :: Int,
    fromInteger minInt2 :: Int
  ]  

maxInt2 = fromIntegral (maxBound :: Int)     * 2 :: Integer
minInt2 = fromIntegral (minBound + 1 :: Int) * 2 :: Integer
