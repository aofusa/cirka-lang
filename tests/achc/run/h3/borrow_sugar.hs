{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE CirkaBorrow #-}
-- ghc: skip
-- max-array-copies: 0
-- The borrow syntax (FEAT-0124): `borrow e as x in body` is
-- `borrow e (\x -> body)`; without `as ... in` it is an ordinary call.
module Main where

import Cirka.Ownership
import Cirka.Borrow
import qualified Cirka.Array.Unboxed as U

main :: IO ()
main = do
  let arr = U.fromListU [3, 1, 2 :: Int]
  case borrow arr as a in do
         n <- U.lengthRef a
         xs <- mapM (U.index a) [0 .. n - 1]
         return (Ur (sum xs)) of
    (arr1, Ur s) -> do
      print s
      case borrowMut arr1 as m in U.writeAt m 0 (s * 10) of
        (arr2, ()) -> case borrow arr2 (\a -> Ur <$> U.index a 0) of
          (arr3, Ur x) -> do
            print x
            print (U.toList (freeze arr3))
