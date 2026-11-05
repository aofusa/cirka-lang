-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T5712.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE GADTs #-}

module Main where

infix 5 :*: 
data T a where
  (:*:) :: Int -> a -> T a
  (:+:) :: Char -> T a -> T a
  deriving( Show )

 -- The Show should print (:*:) infix, but (:+:) prefix, 
 -- since it lacks a fixity declaration

main = print ('x' :+: (3 :*: True))
