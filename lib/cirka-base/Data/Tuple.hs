-- | Functions on pairs (base's Data.Tuple).
module Data.Tuple (fst, snd, curry, uncurry, module Data.Tuple) where

import Cirka.Base

swap :: (a, b) -> (b, a)
swap (a, b) = (b, a)
