{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE KindSignatures #-}
-- | Uniqueness (FEAT-0121, 02-language-extensions.md §3): `Unique a` is the
-- only reference to a value. Its kind is `LType`, so it is bound with
-- multiplicity one only (U-Bind) and never copied; `freeze` gives up the
-- uniqueness. The representation is that of `a` (the newtype is erased).
module Cirka.Ownership
  ( Unique
  , unique
  , freeze
  , Consumable (..)
  , Dupable (..)
  , Movable (..)
  , Ur (..)
  ) where

import Cirka.Base
import Prelude.Linear (Consumable (..), Dupable (..), Movable (..), Ur (..))
import Unsafe.Linear (toLinear)

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
