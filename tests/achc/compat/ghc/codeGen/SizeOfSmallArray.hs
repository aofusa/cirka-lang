-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/SizeOfSmallArray.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash, UnboxedTuples #-}

module Main ( main ) where

import GHC.Exts
import GHC.ST

main = putStr
       (test_sizeofArray
        ++ "\n" ++ test_sizeofMutableArray
        ++ "\n"
       )

test_sizeofArray :: String
test_sizeofArray = flip shows "\n" $ runST $ ST $ \ s# -> go 0 [] s#
  where
    go i@(I# i#) acc s#
        | i < 1000 = case newSmallArray# i# 0 s# of
            (# s2#, marr# #) -> case unsafeFreezeSmallArray# marr# s2# of
                (# s3#, arr# #) -> case sizeofSmallArray# arr# of
                    j# -> go (i+1) ((I# j#):acc) s3#
        | otherwise = (# s#, reverse acc #)

test_sizeofMutableArray :: String
test_sizeofMutableArray = flip shows "\n" $ runST $ ST $ \ s# -> go 0 [] s#
  where
    go i@(I# i#) acc s#
        | i < 1000 = case newSmallArray# i# 0 s# of
            (# s2#, marr# #) -> case sizeofSmallMutableArray# marr# of
                    j# -> go (i+1) ((I# j#):acc) s2#
        | otherwise = (# s#, reverse acc #)
