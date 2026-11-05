{-# LANGUAGE RankNTypes #-}
-- Escaping borrows (FEAT-0124, E0320): the borrow itself, a function using
-- it and a computation using it cannot leave `borrow`; the rigid variable
-- of another rank-2 argument cannot escape either (E0604).
module Main where

import Unsafe.Borrow

leakRef :: Int -> (Int, Ref r Int)
leakRef n = borrow n (\a -> pureB a)

leakFun :: Int -> (Int, Int -> Borrow r Int)
leakFun n = borrow n (\a -> pureB (\_ -> size a))

leakComp :: Int -> (Int, Borrow r Int)
leakComp n = borrow n (\a -> pureB (size a))

-- fine: a value read through the borrow
ok :: Int -> (Int, Int)
ok n = borrow n (\a -> size a)

newtype Box s = Box Int

withBox :: (forall s. Box s -> b) -> b
withBox f = f (Box 0)

leakBox :: Int
leakBox = case withBox (\b -> b) of
  Box n -> n
