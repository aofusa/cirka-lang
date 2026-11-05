-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/arith015.hs (BSD-3-Clause; see LICENSE in this directory).
main = do
  print (map read strange_nums :: [Float]) 
  print (map read strange_nums :: [Double]) 
  where
   strange_nums = ["Infinity","NaN", "-Infinity"]
