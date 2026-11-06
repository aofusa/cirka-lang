{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- max-array-copies: 0
-- `newArray` keeps the zeroed memory for elements that are all zero bits
-- (FEAT-0068). This must not change its meaning: `-0.0` is not zero bits, and
-- an array without elements never evaluates the element.
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as A

ints :: Int -> Int -> [Int]
ints n x = A.toList (freeze (A.newArray n x))

dbls :: Int -> Double -> [Double]
dbls n x = A.toList (freeze (A.newArray n x))

-- The element is known only at run time.
dynamic :: Int -> Int
dynamic x = x - x

main :: IO ()
main = do
  print (ints 3 0, ints 3 7, ints 2 (dynamic 5))
  print (dbls 2 0, dbls 2 1.5)
  print (map isNegativeZero (dbls 2 0), map isNegativeZero (dbls 2 (-0.0)))
  print (A.toList (freeze (A.newArray 2 '\0')), A.toList (freeze (A.newArray 2 'x')))
  print (A.toList (freeze (A.newArray 2 False)), A.toList (freeze (A.newArray 2 True)))
  print (A.size (freeze (A.newArray 0 (undefined :: Int))))
  print (A.size (freeze (A.newArray (-1) (undefined :: Int))))
  -- Large enough for the allocator's large-object path; writes still work.
  let big = freeze (A.write (A.newArray 100000 (0 :: Int)) 99999 5)
  print (big A.! 0, big A.! 50000, big A.! 99999, A.size big)
