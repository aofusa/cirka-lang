-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun-foldable1.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE DeriveFunctor, DeriveFoldable #-}

module Main where

import Prelude hiding (sum)
import Data.Foldable

-- Derive Foldable for a simple data type

data List a = Nil | Cons a (List a)
    deriving (Functor,Foldable,Show)

someList = Cons 1 (Cons 1 (Cons 2 (Cons 3 Nil)))

main = print (sum someList)
