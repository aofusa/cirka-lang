-- foldl', scanl, scanr, unfoldr, intercalate, transpose, zip3
import Data.List

main :: IO ()
main = do
  print (foldl' (+) 0 [1 .. 1000 :: Int])
  print (scanl (+) 0 [1 .. 6 :: Int])
  print (scanl1 max [3, 1, 4, 1, 5, 9, 2, 6 :: Int])
  print (scanr (+) 0 [1 .. 5 :: Int])
  print (scanr1 (+) [1 .. 5 :: Int])
  print (unfoldr (\n -> if n > 100 then Nothing else Just (n, n * 2)) (1 :: Int))
  print (unfoldr (\(a, b) -> if a > 100 then Nothing else Just (a, (b, a + b))) (0 :: Int, 1))
  putStrLn (intercalate ", " ["one", "two", "three"])
  print (intersperse '-' "abcde")
  print (transpose [[1, 2, 3], [4, 5], [6 :: Int]])
  print (transpose ["abc", "de", "f"])
  print (zip3 [1 :: Int ..] "abc" [True, False, True])
  print (unzip3 [(1 :: Int, 'a', "x"), (2, 'b', "y")])
  print (zipWith3 (\a b c -> a + b * c) [1, 2, 3] [4, 5, 6] [7, 8, 9 :: Int])
  print (iterate (* 3) 1 !! 10 :: Int)
  print (takeWhile (< 40) (map (^ 2) [1 :: Int ..]))
  print (concatMap show [1 .. 10 :: Int])
