-- | Functors (base's Data.Functor).
module Data.Functor (Functor(..), (<$>), void, module Data.Functor) where

import Cirka.Base

infixl 4 $>
infixl 1 <&>

($>) :: Functor f => f a -> b -> f b
($>) = flip (<$)

(<&>) :: Functor f => f a -> (a -> b) -> f b
as <&> f = f <$> as

