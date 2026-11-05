-- Semigroup and Monoid instances for user types
import Data.Foldable (foldMap)

newtype MaxI = MaxI Int deriving (Show, Eq)
instance Semigroup MaxI where
  MaxI a <> MaxI b = MaxI (max a b)
instance Monoid MaxI where
  mempty = MaxI minBound

data Stats = Stats Int Int deriving (Show, Eq)
instance Semigroup Stats where
  Stats c1 s1 <> Stats c2 s2 = Stats (c1 + c2) (s1 + s2)
instance Monoid Stats where
  mempty = Stats 0 0

newtype Poly = Poly [Int] deriving (Show, Eq)
instance Semigroup Poly where
  Poly a <> Poly b = Poly (zipLong a b)
    where zipLong (x:xs) (y:ys) = (x + y) : zipLong xs ys
          zipLong xs [] = xs
          zipLong [] ys = ys
instance Monoid Poly where
  mempty = Poly []

stat :: Int -> Stats
stat x = Stats 1 x

main :: IO ()
main = do
  print (mconcat (map MaxI [3, 9, 2]))
  print (mempty :: MaxI)
  print (foldMap stat [4, 5, 6, 7])
  print (mconcat [Poly [1, 2], Poly [10], Poly [0, 0, 5]])
  print (Stats 1 2 <> Stats 3 4 <> mempty)
  print ([1, 2 :: Int] <> [3] <> mempty)
  print (mconcat ["ab", "cd", "ef"])
  print (foldMap (\x -> [x, x * 2]) [1, 2, 3 :: Int])
  print ((Stats 1 1 <> Stats 2 2) <> Stats 3 3 == Stats 1 1 <> (Stats 2 2 <> Stats 3 3))
  print (compare (1 :: Int, 'a') (1, 'b') <> compare 'z' 'a')
