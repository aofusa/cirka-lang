-- | Monads (base's Control.Monad).
module Control.Monad
  ( Functor(..), Monad(..), MonadFail(..), MonadPlus(..)
  , mapM, mapM_, forM, forM_, sequence, sequence_, (=<<), void
  , module Control.Monad
  ) where

import Cirka.Base

infixr 1 >=>, <=<
infixl 4 <$!>

(>=>) :: Monad m => (a -> m b) -> (b -> m c) -> a -> m c
f >=> g = \x -> f x >>= g

(<=<) :: Monad m => (b -> m c) -> (a -> m b) -> a -> m c
(<=<) = flip (>=>)

forever :: Applicative f => f a -> f b
-- Recursive function rather than GHC's knot (no cyclic data without a GC).
forever a = a *> forever a

join :: Monad m => m (m a) -> m a
join x = x >>= id

msum :: (Foldable t, MonadPlus m) => t (m a) -> m a
msum = foldr mplus mzero

mfilter :: MonadPlus m => (a -> Bool) -> m a -> m a
mfilter p ma = do
  a <- ma
  if p a then return a else mzero

filterM :: Applicative m => (a -> m Bool) -> [a] -> m [a]
filterM p = foldr (\x -> liftA2 (\flg -> if flg then (x :) else id) (p x)) (pure [])

mapAndUnzipM :: Applicative m => (a -> m (b, c)) -> [a] -> m ([b], [c])
mapAndUnzipM f xs = unzip <$> traverse f xs

zipWithM :: Applicative m => (a -> b -> m c) -> [a] -> [b] -> m [c]
zipWithM f xs ys = sequenceA (zipWith f xs ys)

zipWithM_ :: Applicative m => (a -> b -> m c) -> [a] -> [b] -> m ()
zipWithM_ f xs ys = sequenceA_ (zipWith f xs ys)

foldM :: (Foldable t, Monad m) => (b -> a -> m b) -> b -> t a -> m b
foldM f z0 xs = go z0 (toList xs)
  where go z [] = return z
        go z (y:ys) = f z y >>= \z' -> go z' ys

foldM_ :: (Foldable t, Monad m) => (b -> a -> m b) -> b -> t a -> m ()
foldM_ f a xs = foldM f a xs >> return ()

replicateM :: Applicative m => Int -> m a -> m [a]
replicateM cnt0 f = loop cnt0
  where loop cnt
          | cnt <= 0 = pure []
          | otherwise = liftA2 (:) f (loop (cnt - 1))

replicateM_ :: Applicative m => Int -> m a -> m ()
replicateM_ cnt0 f = loop cnt0
  where loop cnt
          | cnt <= 0 = pure ()
          | otherwise = f *> loop (cnt - 1)

guard :: Alternative f => Bool -> f ()
guard True = pure ()
guard False = empty

when :: Applicative f => Bool -> f () -> f ()
when p s = if p then s else pure ()

unless :: Applicative f => Bool -> f () -> f ()
unless p s = if p then pure () else s

liftM :: Monad m => (a1 -> r) -> m a1 -> m r
liftM f m1 = do { x1 <- m1; return (f x1) }

liftM2 :: Monad m => (a1 -> a2 -> r) -> m a1 -> m a2 -> m r
liftM2 f m1 m2 = do { x1 <- m1; x2 <- m2; return (f x1 x2) }

ap :: Monad m => m (a -> b) -> m a -> m b
ap m1 m2 = do { x1 <- m1; x2 <- m2; return (x1 x2) }

(<$!>) :: Monad m => (a -> b) -> m a -> m b
f <$!> m = do
  x <- m
  let z = f x
  z `seq` return z
