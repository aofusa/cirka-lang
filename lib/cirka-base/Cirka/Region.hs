{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE RankNTypes #-}
-- | Regions (FEAT-0127, 02-language-extensions.md §6): an arena whose
-- contents are freed all at once when `withRegion` ends.
--
-- `allocIn` / `newRefIn` store a value in a cell of the arena and return a
-- handle (`In r a` / `RefIn r a`). Handles are not reference counted, so
-- values may refer to each other through handles in any shape — cycles
-- included (knot-tying, graphs) — and still be freed with the region,
-- without the cycle collector. Handles are only dereferenced in `RIO r`,
-- which only `withRegion` runs; `r` does not occur in its result, so
-- neither a handle nor an action using it can escape (E0320).
module Cirka.Region
  ( Arena
  , RIO
  , In
  , RefIn
  , withRegion
  , allocIn
  , readIn
  , newRefIn
  , readRefIn
  , writeRefIn
  , modifyRefIn
  , liftIO
  ) where

import Cirka.Base
import Cirka.Prim (Region, primRegionNew, primRegionFree, primRegionCell, primRegionGet, primRegionSet)

-- | The arena of region `r`.
newtype Arena r = Arena Region

-- | Computations inside region `r` (IO actions that may use its handles).
newtype RIO r a = RIO (IO a)

-- | An immutable value stored in region `r`.
newtype In r a = In Int

-- | A mutable reference stored in region `r`.
newtype RefIn r a = RefIn Int

instance Functor (RIO r) where
  fmap f (RIO m) = RIO (fmap f m)

instance Applicative (RIO r) where
  pure x = RIO (return x)
  RIO f <*> RIO x = RIO (f <*> x)
  liftA2 f (RIO x) (RIO y) = RIO (liftA2 f x y)
  RIO x *> RIO y = RIO (x *> y)

instance Monad (RIO r) where
  RIO m >>= k = RIO (m >>= \x -> runRIO (k x))
  return x = RIO (return x)

runRIO :: RIO r a -> IO a
runRIO (RIO m) = m

-- | Runs `f` in a fresh region, then frees the region and everything it
-- holds (its values are dropped; values still used elsewhere stay alive).
withRegion :: (forall r. Arena r -> RIO r a) -> IO a
withRegion f = do
  reg <- primRegionNew
  x <- runRIO (f (Arena reg))
  primRegionFree reg
  return x

-- | Stores `x` in the region.
allocIn :: Arena r -> a -> RIO r (In r a)
allocIn (Arena reg) x = RIO (fmap In (primRegionCell reg x))

-- | The value stored by `allocIn`.
readIn :: In r a -> RIO r a
readIn (In c) = RIO (primRegionGet c)

-- | A mutable reference in the region, holding `x`.
newRefIn :: Arena r -> a -> RIO r (RefIn r a)
newRefIn (Arena reg) x = RIO (fmap RefIn (primRegionCell reg x))

readRefIn :: RefIn r a -> RIO r a
readRefIn (RefIn c) = RIO (primRegionGet c)

writeRefIn :: RefIn r a -> a -> RIO r ()
writeRefIn (RefIn c) x = RIO (primRegionSet c x)

modifyRefIn :: RefIn r a -> (a -> a) -> RIO r ()
modifyRefIn r f = readRefIn r >>= \x -> writeRefIn r (f x)

-- | Runs an IO action inside a region.
liftIO :: IO a -> RIO r a
liftIO = RIO
