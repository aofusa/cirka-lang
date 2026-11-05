-- sortBy, sortOn, groupBy, nub, partition, group, insert, delete
import Data.List
import Data.Ord (comparing)
import Data.Function (on)

data P = P String Int deriving (Show, Eq)

main :: IO ()
main = do
  let xs = [5, 3, 8, 1, 9, 2, 8, 3 :: Int]
  print (sortBy (comparing negate) xs)
  print (sortOn (`mod` 3) xs)
  print (sort xs)
  print (nub xs)
  print (partition even xs)
  print (groupBy ((==) `on` (`div` 3)) (sort xs))
  print (map (\g -> (head g, length g)) (group (sort xs)))
  let ps = [P "bob" 30, P "alice" 25, P "carol" 30, P "dave" 25]
  print (sortBy (comparing (\(P _ a) -> a) <> comparing (\(P n _) -> n)) ps)
  print (sortOn (\(P n _) -> n) ps)
  print (insert 4 [1, 3, 5, 7 :: Int])
  print (delete 8 xs)
  print (xs \\ [8, 3])
  print (union [1, 2, 3] [3, 4, 5 :: Int], intersect [1, 2, 3, 4] [2, 4, 6 :: Int])
  print (maximumBy (comparing snd) (zip [0 :: Int ..] xs))
  print (sortBy (flip compare) "hello world")
