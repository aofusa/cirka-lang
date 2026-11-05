-- foldr vs foldl building, on, comparing, folds producing lists/functions
import Data.Function (on)
import Data.Ord (comparing)
import Data.List (sortBy, groupBy, foldl')

main :: IO ()
main = do
  print (foldr (\x acc -> x : acc) [] [1, 2, 3 :: Int])
  print (foldl (\acc x -> x : acc) [] [1, 2, 3 :: Int])
  putStrLn (foldr (\x acc -> "(" ++ show x ++ " + " ++ acc ++ ")") "0" [1, 2, 3 :: Int])
  putStrLn (foldl (\acc x -> "(" ++ acc ++ " + " ++ show x ++ ")") "0" [1, 2, 3 :: Int])
  print (foldr (-) 0 [10, 5, 2 :: Int], foldl (-) 0 [10, 5, 2 :: Int])
  print (foldr1 (-) [10, 5, 2 :: Int], foldl1 (-) [10, 5, 2 :: Int])
  print (foldr (\x acc -> if x > 3 then [x] else x : acc) [] [1 .. 10 :: Int])
  print (take 5 (foldr (\x acc -> x : acc) [] [1 .. 100 :: Int]))
  print (((==) `on` (`mod` 10)) 13 (23 :: Int))
  let ws = ["apple", "fig", "banana", "kiwi", "plum", "cherry"]
  print (sortBy (comparing length) ws)
  print (map (map snd) (groupBy ((==) `on` fst) (sortBy (comparing fst) [(length w, w) | w <- ws])))
  print (maximum (map length ws), minimum ws)
  print (foldl' (flip (:)) [] "abc")
  print (and [], or [], all even [2, 4 :: Int], any odd [2, 4 :: Int])
  print (compare 1 (2 :: Int), compare "b" "a", comparing snd (1 :: Int, 'a') (0, 'b'))
