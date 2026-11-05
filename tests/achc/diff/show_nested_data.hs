-- derived Show for nested data, negatives, infix constructors, records
module Main where

infixl 6 :+:
infixl 7 :*:

data T = Int :+: Int deriving (Show, Eq)

data E = Lit Int | E :*: E | Neg E | Pair T T deriving (Show, Eq)

data R = R { name :: String, val :: Int, ratio :: Double } deriving (Show, Eq)

data W = W (Maybe Int) [Int] (Int, Int) deriving Show

newtype N = N { unN :: Int } deriving Show

data Op = Plus | Minus deriving (Show, Eq, Ord)

main :: IO ()
main = do
  print ((-1) :+: (-2))
  print (3 :+: 4)
  print (Lit (-3))
  print (Lit 1 :*: Lit (-2))
  print (Lit 1 :*: Lit 2 :*: Lit 3)
  print (Neg (Lit (-5)))
  print (Pair (1 :+: 2) ((-3) :+: 4))
  print (R { name = "x", val = -3, ratio = -1.5 })
  print [R "a" 1 0.5, R "b" (-2) (-0.25)]
  print (W (Just (-2)) [-1, 0, 1] (-5, 6))
  print (N (-7))
  print (Just (N 3))
  print [(1 :: Int, -2 :: Int), (-3, 4)]
  print (Just (-2.5 :: Double), Left (-1) :: Either Int Int)
  print [Just (Just (-1 :: Int)), Nothing]
  print (Just (3 :+: 4))
  print (showsPrec 7 (1 :+: 2) "")
  print (showsPrec 6 (1 :+: 2) "")
  print [Plus, Minus]
  print (compare Plus Minus, Plus == Minus)
  print (minimum [3, -1, 2 :: Int], (-2) ^ (3 :: Int) :: Int)
