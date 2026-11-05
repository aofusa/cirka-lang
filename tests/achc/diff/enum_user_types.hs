-- Enum and Bounded on user enums: full ranges, succ/pred, stepped ranges
module Main where

data Day = Mon | Tue | Wed | Thu | Fri | Sat | Sun
  deriving (Show, Eq, Ord, Enum, Bounded)

data Dir = N | E | S | W deriving (Show, Eq, Enum, Bounded)

data Level = Low | Mid | High deriving (Show, Eq, Ord, Enum, Bounded)

next :: (Eq a, Enum a, Bounded a) => a -> a
next x = if x == maxBound then minBound else succ x

allValues :: (Enum a, Bounded a) => [a]
allValues = [minBound .. maxBound]

main :: IO ()
main = do
  print (allValues :: [Day])
  print (allValues :: [Dir])
  print (allValues :: [Bool])
  print (allValues :: [Ordering])
  print (minBound :: Day, maxBound :: Day)
  print (succ Mon, pred Sun, succ Wed)
  print (map fromEnum [Mon ..])
  print (toEnum 3 :: Day)
  print [Mon, Wed ..]
  print [Sun, Fri ..]
  print [Tue ..]
  print [Tue .. Fri]
  print [Fri .. Tue]
  print [Sat, Thu .. Mon]
  print (map next [N, E, S, W])
  print (map next [Mon, Sun])
  print (zip (allValues :: [Level]) [1 :: Int ..])
  print (length (allValues :: [Day]))
  print (maximum [Low, High, Mid], minimum [High, Mid])
  print (fromEnum (maxBound :: Char), fromEnum False, fromEnum GT)
  print (minBound :: Int, maxBound :: Int)
  print (minBound :: (Bool, Dir), maxBound :: (Bool, Level))
  print ((toEnum 0 :: Level) == Low)
