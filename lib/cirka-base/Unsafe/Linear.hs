{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
-- | Escape hatches from the linearity check (linear-base's Unsafe.Linear).
-- The multiplicity variable `p` is checked as `One` in the bodies below, so
-- these definitions pass the checker; their callers lose the guarantee.
-- Multiplicities are erased before Core, so all of these are the identity
-- at run time.
module Unsafe.Linear (toLinear, toLinear2) where

import Cirka.Base

toLinear :: (a %p -> b) %1 -> a %1 -> b
toLinear f x = f x

toLinear2 :: (a %p -> b %q -> c) %1 -> a %1 -> b %1 -> c
toLinear2 f x y = f x y
