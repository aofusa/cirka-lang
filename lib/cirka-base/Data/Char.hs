-- | Characters (base's Data.Char; character classes for ASCII and
-- Latin-1, the rest of Unicode is treated as letters).
module Data.Char (Char, String, module Data.Char) where

import Cirka.Base
import Cirka.Prim

ord :: Char -> Int
ord = primCharOrd

chr :: Int -> Char
chr n
  | n < 0 || n > 1114111 = errorWithoutStackTrace "Prelude.chr: bad argument"
  | otherwise = primCharChr n

isSpace :: Char -> Bool
isSpace = cirkaIsSpace

isDigit, isOctDigit, isHexDigit :: Char -> Bool
isDigit c = c >= '0' && c <= '9'
isOctDigit c = c >= '0' && c <= '7'
isHexDigit c = isDigit c || (c >= 'A' && c <= 'F') || (c >= 'a' && c <= 'f')

isAscii, isLatin1, isControl, isPrint, isAsciiUpper, isAsciiLower :: Char -> Bool
isAscii c = ord c < 128
isLatin1 c = ord c < 256
isControl c = let n = ord c in n < 32 || (n >= 127 && n < 160)
isPrint c = not (isControl c)
isAsciiUpper c = c >= 'A' && c <= 'Z'
isAsciiLower c = c >= 'a' && c <= 'z'

isUpper, isLower, isAlpha, isLetter, isAlphaNum, isPunctuation, isSymbol, isSeparator :: Char -> Bool
isUpper c = isAsciiUpper c || (let n = ord c in (n >= 192 && n <= 222 && n /= 215))
isLower c = isAsciiLower c || (let n = ord c in (n >= 223 && n <= 255 && n /= 247) || n == 181 || n == 170 || n == 186)
isAlpha c = isUpper c || isLower c || ord c > 255
isLetter = isAlpha
isAlphaNum c = isAlpha c || isDigit c
isPunctuation c = c `elem` "!\"#%&'()*,-./:;?@[\\]_{}" || ord c `elem` [161, 167, 171, 182, 183, 187, 191]
isSymbol c = c `elem` "$+<=>^`|~" || ord c `elem` [162, 163, 164, 165, 166, 168, 169, 172, 174, 175, 176, 177, 180, 184, 215, 247]
isSeparator c = c == ' ' || ord c == 160

toUpper, toLower :: Char -> Char
toUpper c
  | isAsciiLower c = chr (ord c - 32)
  | let n = ord c in n >= 224 && n <= 254 && n /= 247 = chr (ord c - 32)
  | otherwise = c
toLower c
  | isAsciiUpper c = chr (ord c + 32)
  | let n = ord c in n >= 192 && n <= 222 && n /= 215 = chr (ord c + 32)
  | otherwise = c

digitToInt :: Char -> Int
digitToInt c
  | isDigit c = ord c - ord '0'
  | c >= 'a' && c <= 'f' = ord c - ord 'a' + 10
  | c >= 'A' && c <= 'F' = ord c - ord 'A' + 10
  | otherwise = errorWithoutStackTrace ("Char.digitToInt: not a digit " ++ show c)

intToDigit :: Int -> Char
intToDigit i
  | i >= 0 && i <= 9 = chr (ord '0' + i)
  | i >= 10 && i <= 15 = chr (ord 'a' + i - 10)
  | otherwise = errorWithoutStackTrace ("Char.intToDigit: not a digit " ++ show i)

showLitChar :: Char -> ShowS
showLitChar = Cirka.Base.showLitChar
