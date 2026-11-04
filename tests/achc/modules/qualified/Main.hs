{-# LANGUAGE NoImplicitPrelude #-}
module Main (main) where

import qualified Data.Stack as S
import Data.Stack (Stack(..), (<:))
import Util hiding (secret)

main = S.size (1 <: 2 <: Empty) `plus` twice 3
  where twice x = x `plus` x
