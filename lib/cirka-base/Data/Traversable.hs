-- | Traversals (base's Data.Traversable).
module Data.Traversable (Traversable(..), forM, mapM, module Data.Traversable) where

import Cirka.Base

for :: (Traversable t, Applicative f) => t a -> (a -> f b) -> f (t b)
for = flip traverse

