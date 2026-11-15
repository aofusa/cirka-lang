-- The pointer types of lib/cirka-base's Cirka.Foreign, for the desugarer
-- tests of foreign imports (FEAT-0047).
module Cirka.Foreign where

newtype Ptr a = CirkaPtr Int

newtype FunPtr a = CirkaFunPtr Int
