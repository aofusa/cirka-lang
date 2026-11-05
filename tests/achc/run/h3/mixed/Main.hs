{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- max-array-copies: 0
-- A program mixing `.hs` and `.chs` modules (FEAT-0125): this Haskell-mode
-- main module imports the Cirka-mode module `Counter`, which imports the
-- Haskell-mode module `Util`. Under the default `ownership = warn`, the
-- unconsumed linear argument of `ignore` is a warning (E0302), not an error.
module Main where

import Cirka.Ownership
import Counter
import Util (label)

ignore :: a %1 -> Int
ignore _ = 0

main :: IO ()
main = do
  putStrLn (describe (tick (tick (tick (newCounter ())))))
  print (sumTo 0 10)
  putStrLn (classify 0)
  putStrLn (classify 5)
  case sorted [3, 1, 2] of
    (xs, Ur n) -> do
      print xs
      print n
  putStrLn (label (ignore 'x'))
