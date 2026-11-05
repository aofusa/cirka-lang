-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T2529.hs (BSD-3-Clause; see LICENSE in this directory).
-- #2529
-- The example below successfully performed the {{{show}}}, but {{{reads}}}
-- returns an empty list. It fails in both GHCi and GHC. It succeeds if you
-- replaces the infix symbol with a name.

module Main where

data A = (:<>:) { x :: Int, y :: Int } deriving (Read, Show)

t :: A
t = 1 :<>: 2

s :: String
s = show t

r :: [(A,String)]
r = reads s

main :: IO ()
main = do putStrLn s
          putStrLn (show r)
