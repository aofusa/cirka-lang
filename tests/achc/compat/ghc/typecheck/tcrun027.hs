-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun027.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE ImplicitParams #-}

-- Killed GHC 5.04.1

module Main where

type CTPar = ([Double],Int)

us :: (?ctPar :: CTPar) => [Double]
us = let (d,_) = ?ctPar in d

main = let ?ctPar = ([3.4],2) in print us 
