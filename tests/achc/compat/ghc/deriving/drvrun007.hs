-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun007.hs (BSD-3-Clause; see LICENSE in this directory).
module Main( main ) where
-- This one crashed Hugs98

data X = X | X :\ X  deriving Show

main = putStrLn (show (X :\ X))
