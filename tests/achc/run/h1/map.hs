-- Higher-order list functions and a user-defined tree.
data Tree a = Leaf | Node (Tree a) a (Tree a)

insert :: Int -> Tree Int -> Tree Int
insert x Leaf = Node Leaf x Leaf
insert x t@(Node l y r)
  | x < y = Node (insert x l) y r
  | x > y = Node l y (insert x r)
  | otherwise = t

toList :: Tree a -> [a]
toList Leaf = []
toList (Node l x r) = toList l ++ [x] ++ toList r

mapTree :: (a -> b) -> Tree a -> Tree b
mapTree _ Leaf = Leaf
mapTree f (Node l x r) = Node (mapTree f l) (f x) (mapTree f r)

depth :: Tree a -> Int
depth Leaf = 0
depth (Node l _ r) = 1 + max (depth l) (depth r)

main :: IO ()
main = do
  let xs = [5, 3, 8, 1, 4, 7, 9, 2, 6]
      t = foldr insert Leaf xs
  mapM_ print (map (* 2) (filter odd xs))
  mapM_ print (toList (mapTree (+ 100) t))
  print (depth t)
  print (length (concatMap (\x -> [x, x]) xs))
  print (foldl (-) 0 xs)
  print (sum (zipWith (*) xs (reverse xs)))
  let (small, big) = span (< 5) [1 .. 9]
  print (length small * 10 + length big)
  print (maybe 0 (+ 1) (lookup 3 (zip xs [10 ..])))
