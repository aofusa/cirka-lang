-- | Reading values from strings (base's Text.Read).
module Text.Read (Read(..), ReadS, reads, read, readParen, lex, module Text.Read) where

import Cirka.Base

readMaybe :: Read a => String -> Maybe a
readMaybe s = case [x | (x, t) <- reads s, ("", "") <- lex t] of
  [x] -> Just x
  _ -> Nothing

readEither :: Read a => String -> Either String a
readEither s = case [x | (x, t) <- reads s, ("", "") <- lex t] of
  [x] -> Right x
  [] -> Left "Prelude.read: no parse"
  _ -> Left "Prelude.read: ambiguous parse"
