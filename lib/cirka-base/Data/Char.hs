-- | Characters (a subset of base's Data.Char for aCHC H1).
module Data.Char where

import Cirka.Prim

ord :: Char -> Int
ord = primCharOrd

chr :: Int -> Char
chr = primCharChr

isDigit, isSpace, isUpper, isLower, isAlpha, isAlphaNum :: Char -> Bool
isDigit c = let n = ord c in n >= 48 && n <= 57
isSpace c = let n = ord c in n == 32 || (n >= 9 && n <= 13)
isUpper c = let n = ord c in n >= 65 && n <= 90
isLower c = let n = ord c in n >= 97 && n <= 122
isAlpha c = isUpper c || isLower c
isAlphaNum c = isAlpha c || isDigit c

toUpper, toLower :: Char -> Char
toUpper c = if isLower c then chr (ord c - 32) else c
toLower c = if isUpper c then chr (ord c + 32) else c

digitToInt :: Char -> Int
digitToInt c = ord c - 48

intToDigit :: Int -> Char
intToDigit n = chr (n + 48)
