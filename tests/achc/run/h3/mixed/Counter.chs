-- Cirka mode (FEAT-0125): `CirkaOwnership`, `LinearTypes`, `BangPatterns`,
-- `LambdaCase` and `ScopedTypeVariables` are on without pragmas.
module Counter (newCounter, tick, describe, sumTo, classify, sorted) where

import Cirka.Ownership
import Cirka.Borrow
import qualified Cirka.Array.Unboxed as U
import Util (label)

newCounter :: () -> Unique Int
newCounter () = unique 0

tick :: Unique Int %1 -> Unique Int
tick u = unique (freeze u + 1)

describe :: Unique Int %1 -> String
describe u = label (freeze u)

sumTo :: Int -> Int -> Int
sumTo !acc n
  | n == 0 = acc
  | otherwise = sumTo (acc + n) (n - 1)

classify :: Int -> String
classify = \case
  0 -> "zero"
  _ -> "some"

-- Swaps the first two elements in place and reads the length through a
-- borrow (the `borrow` syntax of CirkaBorrow).
sorted :: [Int] -> ([Int], Ur Int)
sorted xs = case borrow (U.swap (U.fromListU xs) 0 1) as a in Ur <$> U.lengthRef a of
  (arr, n) -> (U.toList (freeze arr), n)
