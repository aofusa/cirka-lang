-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T17744.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE OverloadedStrings #-}

module Main where

import T17744A

main :: IO ()
main = print $ completeResults $ feed "f" $ parse uriScheme

uriScheme :: Format (Parser LeftBiasedLocal) Maybe
uriScheme = satisfy_ mytake

ipV4address :: Format (Parser LeftBiasedLocal) Maybe
ipV4address = satisfy_ mytake2
