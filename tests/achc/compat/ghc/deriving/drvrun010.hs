-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun010.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

data Test = Test { field :: Int } deriving (Eq,Show,Read)

main = putStrLn $
       if read (show (Test {field=(-1)})) == Test (-1)
       then "works" else "not"

-- The point here is that if 'show' generates
--      Test { field=-1 }
-- the lexer things the '=-' is one lexeme, which does not work

