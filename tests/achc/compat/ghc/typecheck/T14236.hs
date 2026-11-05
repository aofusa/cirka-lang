-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T14236.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE MagicHash #-}
import GHC.Exts
import Type.Reflection

main = do
    case typeRep @(Int -> Char) of
      App a b -> print (a, b)

    case typeRep @(Int# -> Char) of
      App a b -> print (a, b)

    case typeRep @(Int# -> Char) of
      App a b -> print $ App a (typeRep @String)
