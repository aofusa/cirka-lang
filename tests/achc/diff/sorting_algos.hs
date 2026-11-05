-- insertion, merge and quick sort compared with Data.List.sort
import Data.List (sort)

insertionSort :: Ord a => [a] -> [a]
insertionSort = foldr ins []
  where ins x [] = [x]
        ins x (y:ys) | x <= y = x : y : ys
                     | otherwise = y : ins x ys

mergeSort :: Ord a => [a] -> [a]
mergeSort [] = []
mergeSort [x] = [x]
mergeSort xs = merge (mergeSort l) (mergeSort r)
  where (l, r) = splitAt (length xs `div` 2) xs

merge :: Ord a => [a] -> [a] -> [a]
merge [] ys = ys
merge xs [] = xs
merge (x:xs) (y:ys)
  | x <= y = x : merge xs (y:ys)
  | otherwise = y : merge (x:xs) ys

quickSort :: Ord a => [a] -> [a]
quickSort [] = []
quickSort (p:xs) = quickSort [x | x <- xs, x < p] ++ [p] ++ quickSort [x | x <- xs, x >= p]

lcg :: Int -> Int
lcg x = (x * 1103515245 + 12345) `mod` 2147483648

main :: IO ()
main = do
  let xs = [5, 3, 9, 1, 5, 8, -2, 0, 7] :: [Int]
  print (insertionSort xs)
  print (mergeSort xs)
  print (quickSort xs)
  print (sort xs)
  let big = map (`mod` 1000) (take 2000 (iterate lcg 42))
  print (mergeSort big == sort big)
  print (quickSort big == sort big)
  print (insertionSort (take 300 big) == sort (take 300 big))
  print (take 10 (mergeSort big))
  print (mergeSort (words "the quick brown fox jumps over the lazy dog"))
