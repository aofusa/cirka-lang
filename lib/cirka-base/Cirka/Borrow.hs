{-# LANGUAGE NoImplicitPrelude #-}
-- | Borrows (FEAT-0124, 02-language-extensions.md §5): `Ref r a` (shared,
-- read-only) and `MutRef r a` (exclusive) borrows of an owner `a`, used
-- only inside the `Borrow r` monad, which `borrow` / `borrowMut` run
-- strictly before giving the owner back. The region variable `r` keeps
-- borrows, and closures using them, from escaping (E0320).
--
-- Operations on borrowed arrays are in Cirka.Array / Cirka.Array.Unboxed
-- (`lengthRef`, `index`, `writeAt`).
module Cirka.Borrow
  ( Ref
  , MutRef
  , Borrow
  , borrow
  , borrowMut
  , borrowIn
  , borrowMutIn
  , asRef
  ) where

import Unsafe.Borrow
