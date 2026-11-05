{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE NoImplicitPrelude #-}
-- | A minimal subset of linear-base's Prelude.Linear (FEAT-0120): linear
-- versions of the function combinators, unrestricted values (`Ur`) and the
-- classes for consuming, duplicating and moving values of ordinary types.
-- Import it next to (or instead of) the Prelude; the names below that the
-- Prelude also defines are ambiguous when both are imported unqualified.
module Prelude.Linear
  ( Ur (..)
  , unur
  , forget
  , id
  , const
  , ($)
  , (&)
  , (.)
  , swap
  , lseq
  , Consumable (..)
  , Dupable (..)
  , Movable (..)
  ) where

import Cirka.Base hiding (id, const, ($), (.))
import Unsafe.Linear (toLinear)

infixr 0 $
infixl 1 &
infixr 9 .
infixr 0 `lseq`

-- | An unrestricted value inside a linear context: matching `Ur x` binds
-- `x` with multiplicity many.
data Ur a = Ur a

unur :: Ur a %1 -> a
unur (Ur a) = a

-- | Uses a linear function where an unrestricted one is expected.
forget :: (a %1 -> b) %1 -> a -> b
forget f x = f x

id :: a %1 -> a
id x = x

const :: a %1 -> b -> a
const x _ = x

($) :: (a %p -> b) %1 -> a %p -> b
f $ x = f x

(&) :: a %p -> (a %p -> b) %1 -> b
x & f = f x

(.) :: (b %1 -> c) %1 -> (a %1 -> b) %1 -> a %1 -> c
(f . g) x = f (g x)

swap :: (a, b) %1 -> (b, a)
swap (x, y) = (y, x)

-- | Consumes the first argument, then returns the second.
lseq :: Consumable a => a %1 -> b %1 -> b
lseq a b = consumeThen (consume a) b

consumeThen :: () %1 -> b %1 -> b
consumeThen () b = b

class Consumable a where
  consume :: a %1 -> ()

class Consumable a => Dupable a where
  dup2 :: a %1 -> (a, a)

class Dupable a => Movable a where
  move :: a %1 -> Ur a

instance Consumable () where
  consume () = ()

instance Dupable () where
  dup2 () = ((), ())

instance Movable () where
  move () = Ur ()

instance Consumable Bool where
  consume True = ()
  consume False = ()

instance Dupable Bool where
  dup2 True = (True, True)
  dup2 False = (False, False)

instance Movable Bool where
  move True = Ur True
  move False = Ur False

instance Consumable Int where
  consume = toLinear (\n -> n `seq` ())

instance Dupable Int where
  dup2 = toLinear (\n -> (n, n))

instance Movable Int where
  move = toLinear Ur

instance Consumable Char where
  consume = toLinear (\c -> c `seq` ())

instance Dupable Char where
  dup2 = toLinear (\c -> (c, c))

instance Movable Char where
  move = toLinear Ur

instance Consumable Double where
  consume = toLinear (\d -> d `seq` ())

instance Dupable Double where
  dup2 = toLinear (\d -> (d, d))

instance Movable Double where
  move = toLinear Ur

instance Consumable a => Consumable [a] where
  consume [] = ()
  consume (x : xs) = consume x `lseq` consume xs

instance (Consumable a, Consumable b) => Consumable (a, b) where
  consume (a, b) = a `lseq` consume b

instance Consumable (Ur a) where
  consume (Ur _) = ()

instance Dupable (Ur a) where
  dup2 (Ur a) = (Ur a, Ur a)

instance Movable (Ur a) where
  move (Ur a) = Ur (Ur a)
