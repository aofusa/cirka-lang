-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T1624.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MultiParamTypeClasses, FunctionalDependencies, FlexibleInstances #-}

module Main where

class Foo a b | a -> b where
    foo :: a -> Maybe b
    foo x = Nothing

    bar :: a -> b

instance Foo (Maybe a) a where
    bar (Just x) = x


main = do { print (foo (Just 'x'))
          ; print (bar (Just 'y')) }
