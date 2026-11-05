-- | Folds over data structures (base's Data.Foldable).
module Data.Foldable
  ( Foldable(..), and, or, any, all, concat, concatMap, notElem
  , mapM_, forM_, sequence_, traverse_, sequenceA_, foldl', find, maximumBy, minimumBy
  , module Data.Foldable
  ) where

import Cirka.Base

for_ :: (Foldable t, Applicative f) => t a -> (a -> f b) -> f ()
for_ = flip traverse_

foldrM :: (Foldable t, Monad m) => (a -> b -> m b) -> b -> t a -> m b
foldrM f z0 xs = cirkaFoldlList c return (toList xs) z0
  where c k x z = f x z >>= k

foldlM :: (Foldable t, Monad m) => (b -> a -> m b) -> b -> t a -> m b
foldlM f z0 xs = cirkaFoldrList c return (toList xs) z0
  where c x k z = f z x >>= k

asum :: (Foldable t, Alternative f) => t (f a) -> f a
asum = foldr (<|>) empty

