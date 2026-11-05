-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun040.hs (BSD-3-Clause; see LICENSE in this directory).
module Main(main) where

data Burble a = B1 { op1 :: a -> Int, op2 :: Int -> a, op3 :: Int}
              | B2 { op2 :: Int -> a, op4 :: Int -> Int }


f1 :: Int -> Burble Int
f1 n = B1 { op1 = \x->x+n, op2 = \x -> x, op3 = n }

f2 :: Burble a -> Int -> Int
f2 r@(B1 {op1 = op1 , op2 = op2 }) n = op1 (op2 n) + op3 r

f3 :: Burble a -> Burble a
f3 x@(B1 {op3=op3}) = x {op3 = op3+1}

main = print (f2 (f3 (f1 3)) 4)
