-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T9390.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash, UnboxedTuples #-}
module Main(main ) where

import GHC.IO (IO (..))
import GHC.Exts

writeB :: MutableArray# RealWorld Char -> IO ()
writeB arr# = IO $ \s0# -> (# writeArray# arr# 0# 'B' s0#, () #)

inlineWriteB :: MutableArray# RealWorld Char -> ()
inlineWriteB arr# =
    case f realWorld# of
        (# _, x #) -> x
  where
    IO f = writeB arr#

test :: IO Char
test = IO $ \s0# ->
  case newArray# 1# 'A' s0# of
    (# s1#, arr# #) ->
      case seq# (inlineWriteB arr#) s1# of
        (# s2#, () #) ->
          readArray# arr# 0# s2#

main :: IO ()
main = test >>= print

