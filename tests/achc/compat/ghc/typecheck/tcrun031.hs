-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun031.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MultiParamTypeClasses, GeneralizedNewtypeDeriving #-}

-- A newtype-deriving test

module Main where

class C a b where
  op :: a -> b -> b
instance C [a] Char where
  op [] x = x
  op _  x = 'z'

newtype T = T Char deriving( Show, C [a] )

main = do { print (op [] 'x')
          ; print (op [] (T 'y')) }
