-- | Simple combinators on functions (base's Data.Function).
module Data.Function (id, const, (.), flip, ($), module Data.Function) where

import Cirka.Base

infixl 0 `on`
infixl 1 &

fix :: (a -> a) -> a
-- Unshared (`f (fix f)`): a cyclic value would need a GC.
fix f = f (fix f)

on :: (b -> b -> c) -> (a -> b) -> a -> a -> c
on op f x y = f x `op` f y

(&) :: a -> (a -> b) -> b
x & f = f x

applyWhen :: Bool -> (a -> a) -> a -> a
applyWhen True f x = f x
applyWhen False _ x = x
