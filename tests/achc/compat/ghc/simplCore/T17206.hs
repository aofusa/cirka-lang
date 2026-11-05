-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T17206.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE NoImplicitPrelude #-}

module Main where

import Prelude       (IO, ($), (.), (>>=), putStrLn)
import System.Exit   (exitFailure)
import Unsafe.Coerce (unsafeCoerce)

sequ :: IO () -> IO a -> IO a
sequ c c' = c >>= \ _ -> c'

main :: IO ()
main =
  unsafeCoerce sequ (putStrLn "Hello,") $
  unsafeCoerce sequ (putStrLn "world!") $
  exitFailure
