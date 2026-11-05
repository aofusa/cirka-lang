{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
-- ghc: skip
-- Linear-only types (FEAT-0121): Unique, a user-declared `LType`, the kind
-- of a data type with a linear-only field, U-Arg and linear binders.
module Main where

import Cirka.Ownership
import Prelude.Linear (lseq)

-- A user-defined linear-only type.
type Token :: LType
data Token = Token Int

-- A field of a linear-only type makes the data type linear-only.
data Pair = Pair Token Int

mint :: Int -> Token
mint n = Token n

spend :: Token %1 -> Int
spend (Token n) = n

-- U-Arg: without a signature, the linear-only arguments are linear.
spendBoth a b = spend a + spend b

swapPair :: Pair %1 -> (Int, Token)
swapPair (Pair t n) = (n, t)

bump :: Unique Int %1 -> Unique Int
bump u = unique (freeze u + 1)

-- A let-bound linear-only value is used once.
twice :: Unique Int %1 -> Unique Int
twice u =
  let v = bump u
   in bump v

instance Consumable Token where
  consume (Token n) = consume n

main :: IO ()
main = do
  let u = unique (40 :: Int)
  print (freeze (twice u))
  print (spend (mint 3))
  print (spendBoth (mint 1) (mint 2))
  case swapPair (Pair (mint 7) 5) of
    (n, t) -> print (n + spend t)
  let t = mint 9
  print (t `lseq` "consumed")
  print (consume (unique 'x'))
