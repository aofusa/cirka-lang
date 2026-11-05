-- | The Either type and operations (base's Data.Either).
module Data.Either (Either(..), either, module Data.Either) where

import Cirka.Base

lefts :: [Either a b] -> [a]
lefts xs = [x | Left x <- xs]

rights :: [Either a b] -> [b]
rights xs = [x | Right x <- xs]

partitionEithers :: [Either a b] -> ([a], [b])
partitionEithers = foldr (either left right) ([], [])
  where left a ~(l, r) = (a : l, r)
        right a ~(l, r) = (l, a : r)

isLeft :: Either a b -> Bool
isLeft (Left _) = True
isLeft (Right _) = False

isRight :: Either a b -> Bool
isRight = not . isLeft

fromLeft :: a -> Either a b -> a
fromLeft _ (Left a) = a
fromLeft a _ = a

fromRight :: b -> Either a b -> b
fromRight _ (Right b) = b
fromRight b _ = b
