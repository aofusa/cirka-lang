{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE KindSignatures #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | Uniqueness (FEAT-0121, 02-language-extensions.md §3): `Unique a` is the
-- only reference to a value. Its kind is `LType`, so it is bound with
-- multiplicity one only (U-Bind) and never copied; `freeze` gives up the
-- uniqueness. The representation is that of `a` (the newtype is erased).
-- `Drop` / `Release` give linear-only types RAII (FEAT-0122,
-- 02-language-extensions.md §4): the compiler inserts `drop x` where `x` is
-- left unused and `release x` at the end of the `IO` block that bound it.
module Cirka.Ownership
  ( Unique
  , unique
  , freeze
  , Drop (..)
  , Release (..)
  , returnL
  , dropThen
  , dropAfter
  , releaseAfter
  , Consumable (..)
  , Dupable (..)
  , Movable (..)
  , Ur (..)
  ) where

import Cirka.Base hiding (drop)
import Prelude.Linear (Consumable (..), Dupable (..), Movable (..), Ur (..))
import Unsafe.Linear (toLinear, unsafeReturnIO)

type Unique :: Type -> LType
newtype Unique a = Unique a

-- | Makes a value unique. Values are immutable until the in-place
-- operations of FEAT-0123, which copy a shared value before updating it
-- (the reference count check of `reuse`); until then this is O(1).
unique :: a -> Unique a
unique a = Unique a

-- | Gives up uniqueness: the value becomes an ordinary shared value.
freeze :: Unique a %1 -> a
freeze (Unique a) = a

instance Consumable (Unique a) where
  consume (Unique a) = toLinear (\_ -> ()) a

-- | Pure finalization: gives memory or off-heap storage back, with no
-- observable effect. Inserted wherever a value is left unused.
class Drop (a :: LType) where
  drop :: a %1 -> ()

-- | Finalization with effects (closing a file, unlocking): inserted at the
-- end of the `IO` block that bound the value, in reverse binding order.
-- Leaving such a value unused in pure code is an error (E0304).
class Release (a :: LType) where
  release :: a %1 -> IO ()

instance Drop (Unique a) where
  drop (Unique a) = toLinear (\_ -> ()) a

-- | `return` for linear-only values.
returnL :: forall (a :: LType). a %1 -> IO a
returnL x = unsafeReturnIO x

-- | `drop x` before `b` (inserted by the compiler).
dropThen :: forall (a :: LType) b. Drop a => a %1 -> b -> b
dropThen x b = case drop x of
  () -> b

-- | Runs the action, then drops `x` (inserted at the end of `IO` blocks).
dropAfter :: forall (a :: LType) r. Drop a => IO r -> a %1 -> IO r
dropAfter act x = do
  r <- act
  dropThen x (return r)

-- | Runs the action, then releases `x` (inserted at the end of `IO`
-- blocks).
releaseAfter :: forall (a :: LType) r. Release a => IO r -> a %1 -> IO r
releaseAfter act x = do
  r <- act
  release x
  return r
