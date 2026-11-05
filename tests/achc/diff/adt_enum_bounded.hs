-- ADTs deriving Show, Eq, Ord, Enum, Bounded
data Day = Mon | Tue | Wed | Thu | Fri | Sat | Sun
  deriving (Show, Eq, Ord, Enum, Bounded)

data Suit = Hearts | Spades deriving (Show, Eq, Ord, Enum, Bounded)
data Card = Card Int Suit deriving (Show, Eq, Ord)
data Shape = Circle Int | Rect Int Int deriving (Show, Eq, Ord)

next :: Day -> Day
next d
  | d == maxBound = minBound
  | otherwise = succ d

main :: IO ()
main = do
  print [minBound .. maxBound :: Day]
  print (map next [Fri, Sat, Sun])
  print (fromEnum Wed, toEnum 4 :: Day)
  print [Mon, Wed ..]
  print (compare Tue Thu, Sat > Fri, maximum [Tue, Sun, Mon])
  print (pred Sun, succ Mon)
  print (Card 3 Hearts < Card 3 Spades, Card 2 Spades < Card 3 Hearts)
  print (Circle 5 < Rect 1 1, Rect 2 3 == Rect 2 3, Circle (-2))
  print [(d, s) | d <- [Sat ..], s <- [minBound .. maxBound :: Suit]]
  print (Rect (-1) 2)
