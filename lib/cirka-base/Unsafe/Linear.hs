{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE KindSignatures #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | Escape hatches from the linearity check (linear-base's Unsafe.Linear).
-- The multiplicity variable `p` is checked as `One` in the bodies below, so
-- these definitions pass the checker; their callers lose the guarantee.
-- Multiplicities are erased before Core, so all of these are the identity
-- at run time.
-- Modules named `Unsafe.*` are not checked for linearity or uniqueness
-- (03-type-system.md §2.5).
module Unsafe.Linear (toLinear, toLinear2, unsafeReturnIO) where

import Cirka.Base

toLinear :: forall (a :: LType) (b :: LType) p. (a %p -> b) %1 -> a %1 -> b
toLinear f x = f x

toLinear2 :: forall (a :: LType) (b :: LType) (c :: LType) p q. (a %p -> b %q -> c) %1 -> a %1 -> b %1 -> c
toLinear2 f x y = f x y

-- | `return` at a linear-only type (`return` itself takes `Type` values).
unsafeReturnIO :: forall (a :: LType). a %1 -> IO a
unsafeReturnIO x = return x
