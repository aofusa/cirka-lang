-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/T7163.hs (BSD-3-Clause; see LICENSE in this directory).

module Main where

main :: IO ()
main = putStrLn $ printFloat 100

printFloat :: Float -> String
printFloat x = f (show (round (x * 10)))
    where f "0" = "0"
          f _   = show (round x)
