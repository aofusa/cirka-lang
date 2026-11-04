{-# LANGUAGE NoImplicitPrelude #-}
module Data.Stack (Stack(..), (<:), size) where

import Util (plus)

infixr 5 <:

data Stack a = Empty | Push a (Stack a)

(<:) :: a -> Stack a -> Stack a
x <: s = Push x s

size :: Stack a -> Stack a
size Empty = Empty
size (Push _ s) = size s
