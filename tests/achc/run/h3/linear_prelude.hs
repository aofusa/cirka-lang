{-# LANGUAGE LinearTypes #-}
-- ghc: skip
-- Prelude.Linear (a subset of linear-base) and aCHC's inferred let
-- multiplicities (03-type-system.md §2.3) and `%0`, which GHC 9.4 does not have.
module Main where

import qualified Prelude.Linear as L
import Prelude.Linear (Ur (..), Consumable (..), Dupable (..), Movable (..), lseq, unur)

-- A resource handle that must be closed exactly once.
data Handle = Handle Int

open :: Int -> Handle
open n = Handle n

close :: Handle %1 -> Ur Int
close (Handle n) = L.move n

write :: Int -> Handle %1 -> Handle
write k (Handle n) = Handle (L.move n `withUr` (+ k))

withUr :: Ur Int %1 -> (Int -> Int) -> Int
withUr (Ur n) f = f n

-- let bindings: the multiplicity is inferred from the use.
pipeline :: Handle %1 -> Ur Int
pipeline h =
  let h1 = write 10 h
      h2 = write 5 h1
   in close h2

-- `%0` (an aCHC addition): the argument is not used at run time.
phantom :: a %0 -> Int
phantom _ = 42

-- A linear function where an unrestricted one is expected (GHC 9.4 needs
-- an eta-expansion).
applyTwice :: (a -> a) -> a -> a
applyTwice f x = f (f x)

-- Linear combinators.
twice :: (Int, Bool) %1 -> (Bool, Int)
twice p = L.swap L.$ L.id p

dupCount :: Int %1 -> (Int, Int)
dupCount n = dup2 n

consumeAll :: [Bool] %1 -> Int %1 -> Int
consumeAll bs n = bs `lseq` n

main :: IO ()
main = do
  print (unur (pipeline (open 1)))
  print (twice (3, True))
  print (dupCount 21)
  print (consumeAll [True, False] 7)
  case L.move (5 :: Int) of
    Ur k -> print (k + k)
  print (consume (True, ()) == ())
  print (phantom (error "never forced" :: Int))
  print (applyTwice L.id 'z', map Just [1 :: Int, 2])
