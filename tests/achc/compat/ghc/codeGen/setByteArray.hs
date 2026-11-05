-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/setByteArray.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE BangPatterns, MagicHash, UnboxedTuples #-}
{-# OPTIONS_GHC -Wall #-}

module Main ( main ) where

import GHC.Base

data BA = BA ByteArray#

-- Checks that setByteArray# works
main :: IO ()
main = do BA ba <- IO $ \s0 ->
                        case newByteArray# 8# s0 of
                        (# !s1, !mba #) ->
                            case setByteArray# mba 0# 8# 65# s1 of -- ASCII 'A'
                            !s2 ->
                                case setByteArray# mba 1# 6# 67# s2 of -- ASCII 'B'
                                !s3 ->
                                    -- N.B. 0-length should be a no-op
                                    case setByteArray# mba 2# 0# 68# s3 of -- ASCII 'C'
                                    !s4 ->
                                        case unsafeFreezeByteArray# mba s4 of
                                        (# s5, ba #) -> (# s5, BA ba #)
          let f (I# i) = putStrLn [C# (indexCharArray# ba i)]
          mapM_ f [0..7]

