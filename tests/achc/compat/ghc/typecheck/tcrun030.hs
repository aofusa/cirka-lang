-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun030.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE UndecidableInstances #-}

-- Test recursive dictionaries

module Main where

data D r = ZeroD | SuccD (r (D r));

instance (Eq (r (D r))) => Eq (D r) where
    ZeroD     == ZeroD     = True
    (SuccD a) == (SuccD b) = a == b
    _         == _         = False;

equalDC :: D [] -> D [] -> Bool;
equalDC = (==);

foo :: D []
foo = SuccD [SuccD [ZeroD], ZeroD]

main = print (foo == foo)
