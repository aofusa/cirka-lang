-- Type classes: superclasses, default methods, polymorphic instances,
-- constrained functions, defaulting and higher-kinded classes.
module Main where

class Shape a where
  area :: a -> Double
  name :: a -> String
  describe :: a -> String
  describe x = name x ++ " with area " ++ show (area x)

data Circle = Circle Double
data Rect = Rect Double Double

instance Shape Circle where
  area (Circle r) = 3 * r * r
  name _ = "circle"

instance Shape Rect where
  area (Rect w h) = w * h
  name _ = "rect"
  describe r = "a " ++ name r

class Container f where
  empty :: f a
  insert :: a -> f a -> f a
  toL :: f a -> [a]

newtype Stack a = Stack [a]

instance Container Stack where
  empty = Stack []
  insert x (Stack xs) = Stack (x : xs)
  toL (Stack xs) = xs

fill :: Container f => [a] -> f a
fill = foldr insert empty

data V2 = V2 Int Int deriving (Show, Eq)

instance Num V2 where
  V2 a b + V2 c d = V2 (a + c) (b + d)
  V2 a b * V2 c d = V2 (a * c) (b * d)
  abs (V2 a b) = V2 (abs a) (abs b)
  signum (V2 a b) = V2 (signum a) (signum b)
  fromInteger n = V2 (fromInteger n) (fromInteger n)
  negate (V2 a b) = V2 (negate a) (negate b)

instance Semigroup V2 where
  (<>) = (+)

instance Monoid V2 where
  mempty = 0

sumSq :: Num a => [a] -> a
sumSq xs = sum (map (\x -> x * x) xs)

pairUp :: (Eq a, Show a) => a -> a -> String
pairUp x y
  | x == y = "same " ++ show x
  | otherwise = show x ++ "/" ++ show y

half :: Fractional a => a -> a
half x = x / 2

fact :: Integral a => a -> a
fact 0 = 1
fact n = n * fact (n - 1)

main :: IO ()
main = do
  putStrLn (describe (Circle 1.5))
  putStrLn (describe (Rect 2 3))
  print (toL (fill [1, 2, 3] :: Stack Int))
  print (V2 1 2 + 3)
  print (mconcat [V2 1 1, V2 2 3, V2 4 5])
  print (sumSq [1, 2, 3], sumSq [0.5, 1.5])
  putStrLn (pairUp 'a' 'a')
  putStrLn (pairUp [1, 2] [2, 1])
  print (half 7, fact 10)
  print (fromIntegral (length "abcd") / 8)
  print (2 ^ 10, 2.5 ^ 2, truncate (10 / 3 :: Double), round 2.5, round 3.5)
  print (div (-7) 2, mod (-7) 2, quot (-7) 2, rem (-7) 2, divMod 7 (-2))
  print (fmap (+ 1) (Just 2), fmap (* 2) [1, 2, 3], (+ 1) <$> Right 3 :: Either String Int)
  print (pure 5 :: Maybe Int, [(+ 1), (* 2)] <*> [10, 20])
  print (Just 3 >>= \x -> if x > 2 then Just (x * 2) else Nothing)
  print (sequence [Just 1, Just 2], sequence [Just 1, Nothing], traverse (\x -> [x, x + 1]) [1, 2])
  print (minimum [3, 1, 2], maximum "hello", elem 3 [1, 2, 3], foldr (:) [] (Just 'x'))
  print (compare (1, 'b') (1, 'a'), max "abc" "abd", [LT ..])
  print (sqrt 2 :: Double, pi :: Double, exp 1, 1 / 0 :: Double, 0.1 + 0.2)
  print (show 1.0e-2, show 12345678.9, show 0.1, show (-2.5), [1.0, 1.5 .. 3])
  print (gcd 12 18, lcm 4 6, even 4, odd 4, subtract 3 10)
  print (mapM (\x -> if x > 0 then Right x else Left ("bad " ++ show x)) [1, -2, 3])
  mapM_ print (zip3 [1 :: Int ..] "ab" [True, False])
  let Just y = lookup 2 [(1, "one"), (2, "two")]
  putStrLn y
