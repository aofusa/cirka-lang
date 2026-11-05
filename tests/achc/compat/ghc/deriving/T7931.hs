-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T7931.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE StandaloneDeriving #-} 
module Main where 
 
data A 
deriving instance Read A 
deriving instance Show A 
 
main = print (read "[]" :: [A])
-- Should successfully read the empty list

