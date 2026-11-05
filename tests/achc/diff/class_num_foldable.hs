-- Num instances for user vector/complex-like types, Foldable on Maybe and lists
module Main where

import Data.Foldable (toList, foldl')

data V2 = V2 Int Int deriving (Eq, Show)

instance Num V2 where
  V2 a b + V2 c d = V2 (a + c) (b + d)
  V2 a b - V2 c d = V2 (a - c) (b - d)
  V2 a b * V2 c d = V2 (a * c) (b * d)
  negate (V2 a b) = V2 (negate a) (negate b)
  abs (V2 a b) = V2 (abs a) (abs b)
  signum (V2 a b) = V2 (signum a) (signum b)
  fromInteger n = V2 (fromInteger n) (fromInteger n)

data Cx = Cx Int Int deriving Eq

instance Show Cx where
  showsPrec p (Cx r i) = showParen (p > 6) (showsPrec 7 r . showString " + " . showsPrec 7 i . showString "i")

instance Num Cx where
  Cx a b + Cx c d = Cx (a + c) (b + d)
  Cx a b - Cx c d = Cx (a - c) (b - d)
  Cx a b * Cx c d = Cx (a * c - b * d) (a * d + b * c)
  negate (Cx a b) = Cx (negate a) (negate b)
  abs z = z
  signum z = z
  fromInteger n = Cx (fromInteger n) 0

main :: IO ()
main = do
  print (V2 1 2 + V2 3 4)
  print (V2 1 2 - 5)
  print (V2 2 3 * 4)
  print (negate (V2 1 (-2)), abs (V2 (-3) 4), signum (V2 (-3) 0))
  print (sum [V2 1 1, V2 2 2, V2 3 3])
  print (product [V2 1 2, V2 3 4])
  print (Cx 1 2 * Cx 3 4)
  print (Cx 0 1 * Cx 0 1)
  print (Just (Cx 1 (-1)))
  print (sum [Cx 1 1, Cx 2 2] + 10)
  print (product (replicate 4 (Cx 0 1)))
  print (sum (Just 3), product (Just 4), sum (Nothing :: Maybe Int), product (Nothing :: Maybe Int))
  print (maximum (Just 5), minimum (Just 'a'))
  print (elem 3 (Just 3), elem 3 (Nothing :: Maybe Int), length (Just 'x'), length (Nothing :: Maybe Int))
  print (foldr (:) [] (Just 1), foldr (+) 0 [1, 2, 3 :: Int], foldl (-) 10 [1, 2, 3 :: Int])
  print (sum [1 .. 10 :: Int], product [1 .. 5 :: Int], maximum [3, 9, 2 :: Int], minimum "hello")
  print (elem 'e' "hello", length "hello", null (Just 1), null [] )
  print (toList (Just 'z'), toList (Nothing :: Maybe Int), foldl' (+) 0 [1 .. 100 :: Int])
  print (concatMap show [1, 2, 3 :: Int], and (Just True), or [False, False], any even (Just 3), all odd [1, 3 :: Int])
