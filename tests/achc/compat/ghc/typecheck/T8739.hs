-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T8739.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}
module Main where
import GHC.Exts

go :: () -> Int#
go () = 0#

main = print (lazy (I# (go $ ())))


