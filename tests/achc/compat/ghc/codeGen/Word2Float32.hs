-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/Word2Float32.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}
module Main (main) where

import GHC.Exts (Double(D#), Float(F#), word2Double#, word2Float#)

main :: IO ()
main = do
    print (D# (word2Double# 0##))
    -- 4294967295 is 2^32 - 1, the largest 32-bit integer, and can be
    -- stored in a 64-bit IEEE floating-point value without loss of
    -- precision.
    print (D# (word2Double# 4294967295##))
    print (F# (word2Float# 0##))
    -- 16777216 is 2^24, which is the largest integer which can be
    -- stored in a 32-bit IEEE floating-point value without loss of
    -- precision
    print (F# (word2Float# 16777216##))
