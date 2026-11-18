{-# LANGUAGE NoImplicitPrelude #-}
-- | The types of `extern "C"` blocks (FEAT-0130, 08-ffi.md §3, §4, §7.4).
--
-- An `extern "C"` block (`CirkaExtern`, on in `.chs`) declares opaque C
-- types and C functions whose types may use:
--
-- * `OwnedPtr T`: a pointer the program must free (linear-only). Taken by
--   a linear argument (`OwnedPtr T %1 -> ...`). `free with h` after a
--   declaration that returns one generates `Release (OwnedPtr T)` through
--   `h` (T an opaque type of the block), so it is closed like any RAII
--   value; otherwise forgetting it is E0302.
-- * `BorrowedPtr T`: a pointer lent by `withBorrowed` / `withBorrowedIO`.
--   The compiler adds the scope (`BorrowedPtr r T`) and makes the result
--   `BorrowIO r t` (or `Borrow r t` for a pure function).
-- * `Nullable p` (`Maybe p`): NULL is `Nothing`.
module Cirka.Extern
  ( OwnedPtr
  , BorrowedPtr
  , Nullable
  , BorrowIO
  , Borrow
  , liftIO
  , liftBorrow
  , withBorrowed
  , withBorrowedIO
  , releaseWith
  ) where

import Cirka.Borrow (Borrow)
import Unsafe.Extern
