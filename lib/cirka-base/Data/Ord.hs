-- | Orderings (base's Data.Ord).
module Data.Ord (Ord(..), Ordering(..), comparing, module Data.Ord) where

import Cirka.Base

-- | Reverses an ordering.
newtype Down a = Down a

getDown :: Down a -> a
getDown (Down a) = a

instance Eq a => Eq (Down a) where
  Down a == Down b = a == b

instance Ord a => Ord (Down a) where
  compare (Down a) (Down b) = compare b a

instance Show a => Show (Down a) where
  showsPrec d (Down a) = showParen (d >= 11) (showString "Down " . showsPrec 11 a)

clamp :: Ord a => (a, a) -> a -> a
clamp (low, high) a = min high (max a low)
