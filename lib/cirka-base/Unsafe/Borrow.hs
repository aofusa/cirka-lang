{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE KindSignatures #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE BangPatterns #-}
-- | The implementation of borrows (FEAT-0124, 02-language-extensions.md §5).
-- Cirka.Borrow exports the types abstractly; operations on borrowed values
-- (e.g. in Cirka.Array) use the constructors from here.
--
-- A borrow is the owner itself (the newtypes are erased); a `Borrow r`
-- computation is an IO action run strictly by `borrow`, before the owner is
-- given back. `r` does not occur in the result type of `borrow`, so neither
-- the borrow nor a closure using it (whose type mentions `Borrow r`) can
-- escape (rank-2 parametricity, as with `runST`).
module Unsafe.Borrow
  ( Ref (..)
  , MutRef (..)
  , Borrow (..)
  , borrow
  , borrowMut
  , borrowIn
  , borrowMutIn
  , asRef
  , refValue
  , mutRefValue
  , borrowIO
  ) where

import Cirka.Base
import Cirka.Ownership (Unique, freeze)
import Cirka.Prim (primRunIO)

-- Declared `Type` although they hold the (maybe linear-only) owner: a
-- borrow may be used any number of times inside its scope.
type Ref :: Type -> LType -> Type
newtype Ref r a = Ref a

type MutRef :: Type -> LType -> Type
newtype MutRef r a = MutRef a

newtype Borrow r a = Borrow (IO a)

instance Functor (Borrow r) where
  fmap f (Borrow m) = Borrow (fmap f m)

instance Applicative (Borrow r) where
  pure x = Borrow (return x)
  Borrow f <*> Borrow x = Borrow (f <*> x)
  liftA2 f (Borrow x) (Borrow y) = Borrow (liftA2 f x y)
  Borrow x *> Borrow y = Borrow (x *> y)

instance Monad (Borrow r) where
  Borrow m >>= k = Borrow (m >>= \x -> runB (k x))
  return x = Borrow (return x)

runB :: Borrow r a -> IO a
runB (Borrow m) = m

-- | Runs `f` with a shared borrow of `x`, then gives `x` back.
borrow :: forall (a :: LType) (b :: LType). a %1 -> (forall r. Ref r a -> Borrow r b) %1 -> (a, b)
borrow x f = let !b = primRunIO (runB (f (Ref x))) in (x, b)

-- | Runs `f` with an exclusive (writable) borrow of `x`, then gives `x` back.
borrowMut :: forall (a :: LType) (b :: LType). a %1 -> (forall r. MutRef r a -> Borrow r b) %1 -> (a, b)
borrowMut x f = let !b = primRunIO (runB (f (MutRef x))) in (x, b)

-- | `borrow` inside another borrow's computation.
borrowIn :: forall (a :: LType) (b :: LType) r. a %1 -> (forall r'. Ref r' a -> Borrow r' b) %1 -> Borrow r (a, b)
borrowIn x f = Borrow (runB (f (Ref x)) >>= \b -> return (x, b))

-- | `borrowMut` inside another borrow's computation.
borrowMutIn :: forall (a :: LType) (b :: LType) r. a %1 -> (forall r'. MutRef r' a -> Borrow r' b) %1 -> Borrow r (a, b)
borrowMutIn x f = Borrow (runB (f (MutRef x)) >>= \b -> return (x, b))

-- | Reads through an exclusive borrow.
asRef :: forall r (a :: LType). MutRef r a -> Ref r a
asRef (MutRef x) = Ref x

-- | The borrowed unique value (shared with its owner: only for reading, or
-- for the in-place stores of borrow operations).
refValue :: Ref r (Unique a) -> a
refValue (Ref u) = freeze u

mutRefValue :: MutRef r (Unique a) -> a
mutRefValue (MutRef u) = freeze u

-- | A borrow operation from an IO action.
borrowIO :: IO a -> Borrow r a
borrowIO = Borrow
