-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T9858d.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE DataKinds #-}
module Main where

import Data.Typeable

data A = A

main = print $ typeRep (Proxy :: Proxy A) == typeRep (Proxy :: Proxy 'A)
