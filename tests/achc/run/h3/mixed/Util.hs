-- A Haskell-mode module imported by the Cirka-mode module `Counter`.
module Util (label) where

label :: Int -> String
label n = "count: " ++ show n
