-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T15436.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

import GHC.Enum

data XXX = AL | AK | AZ | AR | CA | CO | CT | DE | FL
    deriving (Enum, Bounded, Show)

data Z = Y | X XXX deriving( Show )

instance Enum Z where
  fromEnum Y     = 0
  fromEnum (X s) = 1 + fromEnum s
  toEnum 0   = Y
  toEnum i   = X (toEnum (i - 1))

instance Bounded Z where
  minBound = Y
  maxBound = X maxBound


main = print [ succ (x :: Z) | x <- [minBound .. pred maxBound] ]
