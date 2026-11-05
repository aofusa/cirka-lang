{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
-- RAII errors (FEAT-0122): a value with only a Release instance left
-- unused in pure code (E0304); a value with neither instance (E0302).
module Main where

import Cirka.Ownership

type File :: LType
data File = File

instance Release File where
  release File = return ()

type Plain :: LType
data Plain = Plain Int

openFile :: Int -> File
openFile _ = File

pureDiscard :: Int -> Int
pureDiscard n = let f = openFile n in n

ignored :: File %1 -> Int
ignored f = 0

noInstance :: Plain %1 -> Int
noInstance p = 1

main :: IO ()
main = return ()
