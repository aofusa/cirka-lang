-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/T7361.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}
import GHC.Exts

main = print $ map f [1,256,65536,minBound,maxBound]

f (I# x#) =
      [ I# (narrow8Int#  (narrow16Int# x#))
      , I# (narrow8Int#  (narrow32Int# x#))
      , I# (narrow16Int# (narrow8Int#  x#))
      , I# (narrow16Int# (narrow32Int# x#))
      , I# (narrow32Int# (narrow8Int#  x#))
      , I# (narrow32Int# (narrow16Int# x#))]
