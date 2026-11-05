-- Functor, Foldable and Traversable instances for a user tree
import Data.Foldable (toList, foldl')

data Tree a = Leaf | Node (Tree a) a (Tree a) deriving Show

instance Functor Tree where
  fmap _ Leaf = Leaf
  fmap f (Node l v r) = Node (fmap f l) (f v) (fmap f r)

instance Foldable Tree where
  foldr _ z Leaf = z
  foldr f z (Node l v r) = foldr f (f v (foldr f z r)) l

instance Traversable Tree where
  traverse _ Leaf = pure Leaf
  traverse f (Node l v r) = Node <$> traverse f l <*> f v <*> traverse f r

fromList :: [a] -> Tree a
fromList [] = Leaf
fromList xs = Node (fromList l) x (fromList r)
  where (l, x:r) = splitAt (length xs `div` 2) xs

main :: IO ()
main = do
  let t = fromList [1 .. 7 :: Int]
  print (toList t)
  print (toList (fmap (* 10) t))
  print (sum t, product t, length t)
  print (maximum t, minimum t, elem 4 t, elem 9 t)
  print (foldl' (flip (:)) [] t)
  print (foldr (\x acc -> x : take 2 acc) [] t)
  print (null t, null (Leaf :: Tree Int))
  print (fmap toList (traverse (\x -> if x > 0 then Just x else Nothing) t))
  print (fmap toList (traverse (\x -> if x > 3 then Nothing else Just x) t))
  print (concatMap show t)
  print (fmap (+ 1) (Node Leaf (-1 :: Int) Leaf))
