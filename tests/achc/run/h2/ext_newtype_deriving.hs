{-# LANGUAGE GeneralizedNewtypeDeriving #-}
-- FEAT-0118: instances of newtypes from their representation type.
module Main where

newtype Age = Age Int deriving (Show, Eq, Ord, Num)

newtype Score = Score Double deriving (Show, Eq, Ord, Num, Fractional)

newtype Log a = Log [a] deriving (Show, Semigroup, Monoid)

newtype Box a = Box (Maybe a) deriving (Show, Functor)

newtype Counter a = Counter a deriving (Show, Eq, Num)

older :: (Num a, Ord a) => a -> a -> a
older x y = max x y + 0

main :: IO ()
main = do
  print (Age 3 + Age 4, older (Age 1) (Age 9))
  print (Score 1.5 / 2, recip (Score 4))
  print (Log [1, 2 :: Int] <> Log [3] <> mempty)
  print (mconcat [Log "ab", Log "c"])
  print (fmap (+ 1) (Box (Just (2 :: Int))), fmap show (Box Nothing :: Box Int))
  print (Counter (2 :: Integer) * 21, negate (Counter (5 :: Int)) == Counter (-5))
  print (sum (map Age [1 .. 10]))
