{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE RankNTypes #-}
-- ghc: skip
-- max-array-copies: 0
-- Borrows (FEAT-0124): reading through a shared borrow, writing in place
-- through an exclusive one, nested borrows; the owner comes back unchanged
-- in identity (no copy). A shareable result is returned in `Ur`.
module Main where

import Cirka.Ownership
import Cirka.Borrow
import Cirka.Array (Array)
import qualified Cirka.Array as A
import qualified Cirka.Array.Unboxed as U

total :: Ref r (Unique (Array Int)) -> Borrow r Int
total a = do
  n <- A.lengthRef a
  xs <- mapM (A.index a) [0 .. n - 1]
  return (sum xs)

double :: MutRef r (Unique (Array Int)) -> Borrow r ()
double m = do
  n <- A.lengthRef (asRef m)
  mapM_ (\i -> A.index (asRef m) i >>= \x -> A.writeAt m i (2 * x)) [0 .. n - 1]

main :: IO ()
main = do
  let arr = A.fromListU [1 .. 5 :: Int]
  case borrow arr (\a -> Ur <$> total a) of
    (arr1, Ur s) -> do
      print s
      case borrowMut arr1 double of
        (arr2, ()) -> case borrow arr2 (\a -> Ur <$> total a) of
          (arr3, Ur s2) -> do
            print s2
            print (A.toList (freeze arr3))
  -- nested: borrow a second array inside the first borrow
  let xs = U.fromListU [10, 20, 30 :: Int]
      ys = U.fromListU [1, 2, 3 :: Int]
  case borrow xs (\a -> borrowIn ys (\b -> Ur <$> mapM (U.index b) [0 .. 2])) of
    (xs', (ys', Ur bs)) -> do
      print bs
      case borrow xs' (\a -> Ur <$> mapM (U.index a) [0 .. 2]) of
        (xs'', Ur as) -> do
          print (zipWith (+) as bs)
          print (U.toList (freeze xs''), U.toList (freeze ys'))
  case borrowMut (U.newArray 3 'a') (\m -> U.writeAt m 1 'z') of
    (cs, ()) -> putStrLn (U.toList (freeze cs))
