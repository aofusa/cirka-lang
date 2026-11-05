-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun012.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE ImplicitParams #-}

-- !!! Implicit parameter test

module Main where

main = do { let ?x = 13 in putStrLn $ show $ foo
          ; let ?x = 14 in putStrLn $ show $ baz () }

foo :: (?x :: Int) => Int
foo = ?x

-- Check that defaulting works too
baz () = ?x

