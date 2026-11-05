-- | Applicative functors (base's Control.Applicative).
module Control.Applicative
  ( Applicative(..), Alternative(..), Functor(..), (<$>), module Control.Applicative
  ) where

import Cirka.Base

liftA :: Applicative f => (a -> b) -> f a -> f b
liftA f a = pure f <*> a

liftA3 :: Applicative f => (a -> b -> c -> d) -> f a -> f b -> f c -> f d
liftA3 f a b c = liftA2 f a b <*> c

optional :: Alternative f => f a -> f (Maybe a)
optional v = Just <$> v <|> pure Nothing
