-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T10104.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}
module Main where

import GHC.Exts

data P = Positives Int# Float# Double# Char# Word# deriving Show
data N = Negatives Int# Float# Double# deriving Show

main = do
  print $ Positives 42# 4.23# 4.23## '4'# 4##
  print $ Negatives -4# -4.0# -4.0##
