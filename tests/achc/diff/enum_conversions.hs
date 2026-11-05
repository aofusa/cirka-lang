-- toEnum/fromEnum, succ/pred, hand-written Enum instance, bounds
module Main where

data Suit = Clubs | Diamonds | Hearts | Spades deriving (Show, Eq)

instance Enum Suit where
  fromEnum Clubs = 0
  fromEnum Diamonds = 1
  fromEnum Hearts = 2
  fromEnum Spades = 3
  toEnum 0 = Clubs
  toEnum 1 = Diamonds
  toEnum 2 = Hearts
  toEnum 3 = Spades
  toEnum _ = error "bad suit"

data Size = Small | Large deriving (Show, Eq, Ord, Enum, Bounded)

main :: IO ()
main = do
  print (map fromEnum "AZaz09")
  print (map toEnum [72, 101, 108, 108, 111] :: String)
  print (toEnum 1 :: Bool, toEnum 2 :: Ordering)
  print (fromEnum 'A', fromEnum True, fromEnum LT)
  print (fromEnum (3.7 :: Double), fromEnum (-3.7 :: Double))
  print (toEnum 65 :: Char)
  print [Clubs .. Hearts]
  print (succ Clubs, pred Spades)
  print (map fromEnum [Clubs, Spades])
  print (map toEnum [3, 2, 1, 0] :: [Suit])
  print (succ 'a', pred 'b', succ (1 :: Int), pred (0 :: Int))
  print (succ 1.5 :: Double, pred 1.5 :: Double)
  print (succ (10 :: Integer), pred (-10 :: Integer))
  print (succ Small, pred Large)
  print (map fromEnum [Small ..])
  print ([minBound .. maxBound] :: [Size])
  print (zip "abc" [Small ..])
  print (enumFromThenTo 10 7 (-5 :: Int))
  print (enumFromThenTo 'e' 'c' 'a')
  print (fromEnum (maxBound :: Int) == maxBound)
  print ((toEnum . (+ 1) . fromEnum) 'a' :: Char)
