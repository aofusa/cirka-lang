-- derived vs hand-written Ord, hand-written Show with showsPrec and precedence
module Main where

import Data.List (sort, sortBy, insert)
import Data.Ord (comparing, Down(..))

data P = P Int String deriving (Eq, Ord, Show)

newtype Rev = Rev Int deriving (Eq, Show)

instance Ord Rev where
  compare (Rev a) (Rev b) = compare b a

data Version = Version [Int] String deriving (Eq, Show)

instance Ord Version where
  compare (Version a _) (Version b _) = compare a b

data Expr = Lit Int | Add Expr Expr | Mul Expr Expr | Neg Expr

instance Show Expr where
  showsPrec p (Lit n) = showsPrec p n
  showsPrec p (Add a b) = showParen (p > 6) (showsPrec 6 a . showString " + " . showsPrec 7 b)
  showsPrec p (Mul a b) = showParen (p > 7) (showsPrec 7 a . showString " * " . showsPrec 8 b)
  showsPrec p (Neg e) = showParen (p > 9) (showString "-" . showsPrec 10 e)

main :: IO ()
main = do
  print (sort [P 2 "b", P 1 "z", P 2 "a", P 1 "c"])
  print (compare (P 1 "a") (P 1 "b"), P 3 "x" < P 2 "y", max (P 1 "q") (P 1 "r"))
  print (sort (map Rev [3, 1, 4, 1, 5, 9, 2, 6]))
  print (maximum (map Rev [3, 1, 4]), minimum (map Rev [3, 1, 4]))
  print (Rev 1 < Rev 2, Rev 1 >= Rev 2, compare (Rev 5) (Rev 5))
  print (sort [Version [1, 10] "b", Version [1, 2] "a", Version [1, 2, 1] "c", Version [0, 9] "d"])
  print (Version [1] "x" == Version [1] "y", compare (Version [1] "x") (Version [1] "y"))
  print (sortBy (comparing Down) [3, 1, 2 :: Int])
  print (sortBy (comparing snd) [(1 :: Int, 'c'), (2, 'a'), (3, 'b')])
  print (insert (Rev 4) (map Rev [9, 5, 3, 1]))
  let e1 = Add (Lit 1) (Mul (Lit 2) (Lit 3))
      e2 = Mul (Add (Lit 1) (Lit 2)) (Lit 3)
      e3 = Add (Add (Lit 1) (Lit 2)) (Add (Lit 3) (Lit 4))
      e4 = Neg (Add (Lit 1) (Lit (-2)))
  print e1
  print e2
  print e3
  print e4
  print (Mul (Lit (-1)) (Neg (Lit 2)))
  print (Just e2)
  print [e1, e4]
  putStrLn (showsPrec 7 e1 "")
  print (Add (Lit 1) (Add (Lit 2) (Lit 3)))
