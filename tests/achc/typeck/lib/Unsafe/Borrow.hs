{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
{-# LANGUAGE RankNTypes #-}
-- The borrow types of lib/cirka-base's Unsafe.Borrow, for the type checker
-- tests (FEAT-0124).
module Unsafe.Borrow where

type Ref :: Type -> LType -> Type
newtype Ref r a = Ref a

newtype Borrow r a = Borrow a

borrow :: forall (a :: LType) (b :: LType). a %1 -> (forall r. Ref r a -> Borrow r b) %1 -> (a, b)
borrow x f = case f (Ref x) of
  Borrow b -> (x, b)

pureB :: b -> Borrow r b
pureB = Borrow

size :: Ref r Int -> Borrow r Int
size (Ref n) = Borrow n
