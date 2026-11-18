{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
{-# LANGUAGE KindSignatures #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE BangPatterns #-}
-- | The implementation of the types of `extern "C"` blocks (FEAT-0130,
-- 08-ffi.md §3, §4, §7.4). Cirka.Extern exports the types abstractly; code
-- that needs the raw pointer (to pass it to a `foreign import`, say) uses
-- the constructors from here.
--
-- All three pointer types are the address (`Ptr a`, an `Int`). A borrowed
-- pointer is the owner's address, used only inside `Borrow r` / `BorrowIO r`
-- computations, which `withBorrowed` / `withBorrowedIO` run before giving
-- the owner back; `r` does not occur in their results, so neither the
-- pointer nor a computation using it can escape (as with `borrow`).
module Unsafe.Extern
  ( OwnedPtr (..)
  , BorrowedPtr (..)
  , Nullable
  , BorrowIO (..)
  , runBorrowIO
  , liftIO
  , liftBorrow
  , withBorrowed
  , withBorrowedIO
  , releaseWith
  ) where

import Cirka.Base
import Cirka.Foreign (Ptr)
import Cirka.Ownership (returnL)
import Cirka.Prim (primRunIO)
import Unsafe.Borrow (Borrow (..))

-- | A C pointer the program owns and must free: linear-only, with no
-- `Drop` or `Release` unless `free with` declares the function that frees
-- it (or a wrapper type defines one).
type OwnedPtr :: Type -> LType
newtype OwnedPtr a = OwnedPtr (Ptr a)

-- | A C pointer lent for the extent of `withBorrowed` / `withBorrowedIO`.
-- Declared `Type`: it may be used any number of times inside its scope.
type BorrowedPtr :: Type -> Type -> Type
newtype BorrowedPtr r a = BorrowedPtr (Ptr a)

-- | A pointer that may be NULL (`Nothing`) at the C boundary.
type Nullable p = Maybe p

-- | IO actions that may use the borrowed pointers of scope `r`. Only
-- `withBorrowedIO` (in `IO`) runs them; `Borrow r` is not enough, since a
-- pure `withBorrowed` or `borrow` runs that.
newtype BorrowIO r a = BorrowIO (IO a)

instance Functor (BorrowIO r) where
  fmap f (BorrowIO m) = BorrowIO (fmap f m)

instance Applicative (BorrowIO r) where
  pure x = BorrowIO (return x)
  BorrowIO f <*> BorrowIO x = BorrowIO (f <*> x)
  liftA2 f (BorrowIO x) (BorrowIO y) = BorrowIO (liftA2 f x y)
  BorrowIO x *> BorrowIO y = BorrowIO (x *> y)

instance Monad (BorrowIO r) where
  BorrowIO m >>= k = BorrowIO (m >>= \x -> runBorrowIO (k x))
  return x = BorrowIO (return x)

runBorrowIO :: BorrowIO r a -> IO a
runBorrowIO (BorrowIO m) = m

-- | Runs an IO action inside a borrow.
liftIO :: IO a -> BorrowIO r a
liftIO = BorrowIO

-- | Runs a pure borrow computation (a pure C function taking a borrowed
-- pointer) inside an IO one.
liftBorrow :: Borrow r a -> BorrowIO r a
liftBorrow (Borrow m) = BorrowIO m

-- | Lends `p` to the pure computation `f`, then gives it back.
withBorrowed :: forall a (b :: LType). OwnedPtr a %1 -> (forall r. BorrowedPtr r a -> Borrow r b) %1 -> (OwnedPtr a, b)
withBorrowed (OwnedPtr p) f = let !b = primRunIO (runB (f (BorrowedPtr p))) in (OwnedPtr p, b)
  where
    runB (Borrow m) = m

-- | Lends `p` to the IO computation `f`, then gives it back.
withBorrowedIO :: forall a b. OwnedPtr a %1 -> (forall r. BorrowedPtr r a -> BorrowIO r b) %1 -> IO (OwnedPtr a, b)
withBorrowedIO (OwnedPtr p) f = runBorrowIO (f (BorrowedPtr p)) >>= \b -> returnL (OwnedPtr p, b)

-- | `release` through the C function `h` that frees the pointer, ignoring
-- its result (the instances that `free with h` generates).
releaseWith :: forall a b. (OwnedPtr a %1 -> IO b) -> OwnedPtr a %1 -> IO ()
releaseWith h p = h p >>= \_ -> return ()
