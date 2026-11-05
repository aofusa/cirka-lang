-- binary search tree and a small expression evaluator
data Tree a = Leaf | Node (Tree a) a (Tree a) deriving Show

insert :: Ord a => a -> Tree a -> Tree a
insert x Leaf = Node Leaf x Leaf
insert x t@(Node l v r)
  | x < v = Node (insert x l) v r
  | x > v = Node l v (insert x r)
  | otherwise = t

member :: Ord a => a -> Tree a -> Bool
member _ Leaf = False
member x (Node l v r)
  | x < v = member x l
  | x > v = member x r
  | otherwise = True

toList :: Tree a -> [a]
toList Leaf = []
toList (Node l v r) = toList l ++ [v] ++ toList r

height :: Tree a -> Int
height Leaf = 0
height (Node l _ r) = 1 + max (height l) (height r)

data Expr = Num Int | Add Expr Expr | Mul Expr Expr | Neg Expr | Div Expr Expr
  deriving (Show, Eq)

eval :: Expr -> Either String Int
eval (Num n) = Right n
eval (Add a b) = (+) <$> eval a <*> eval b
eval (Mul a b) = (*) <$> eval a <*> eval b
eval (Neg a) = negate <$> eval a
eval (Div a b) = do
  x <- eval a
  y <- eval b
  if y == 0 then Left "div by zero" else Right (x `div` y)

main :: IO ()
main = do
  let t = foldr insert Leaf [5, 2, 8, 1, 9, 3, 5 :: Int]
  print (toList t)
  print (map (`member` t) [3, 4, 9])
  print (height t)
  print (insert 2 (insert 1 Leaf :: Tree Int))
  print (eval (Add (Num 2) (Mul (Num 3) (Num 4))))
  print (eval (Neg (Num 5)))
  print (eval (Div (Num 7) (Add (Num 1) (Neg (Num 1)))))
  print (eval (Div (Num (-7)) (Num 2)))
  print (Add (Num (-1)) (Num 2))
