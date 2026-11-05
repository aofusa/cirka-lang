-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun001.hs (BSD-3-Clause; see LICENSE in this directory).
-- Test newtype derived instances

newtype Age = MkAge Int deriving (Eq, Show)

instance Num Age where
  (+) (MkAge a) (MkAge b) = MkAge (a+b)
  (*)         = undefined
  negate      = undefined
  abs         = undefined
  signum      = undefined
  fromInteger = undefined

main = print (MkAge 3 + MkAge 5)
