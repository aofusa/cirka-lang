-- user classes with default methods, superclasses and constrained functions
class Show a => Describe a where
  describe :: a -> String
  describe x = "<" ++ show x ++ ">"
  name' :: a -> String

class Describe a => Loud a where
  shout :: a -> String
  shout x = describe x ++ "!"

data Dog = Dog deriving Show
data Cat = Cat Int deriving Show

instance Describe Dog where
  name' _ = "dog"

instance Describe Cat where
  describe (Cat n) = "cat#" ++ show n
  name' _ = "cat"

instance Loud Dog
instance Loud Cat where
  shout c = name' c ++ " says meow"

class Shape a where
  area :: a -> Int
  perimeter :: a -> Int
  summary :: a -> String
  summary s = show (area s) ++ "/" ++ show (perimeter s)

newtype Sq = Sq Int
data Rc = Rc Int Int

instance Shape Sq where
  area (Sq n) = n * n
  perimeter (Sq n) = 4 * n

instance Shape Rc where
  area (Rc a b) = a * b
  perimeter (Rc a b) = 2 * (a + b)
  summary r = "rect " ++ show (area r)

totalArea :: Shape a => [a] -> Int
totalArea = sum . map area

both :: (Loud a, Loud b) => a -> b -> [String]
both a b = [shout a, shout b]

main :: IO ()
main = do
  putStrLn (describe Dog)
  putStrLn (describe (Cat 3))
  putStrLn (name' Dog ++ " " ++ name' (Cat 1))
  print (both Dog (Cat 7))
  putStrLn (summary (Sq 4))
  putStrLn (summary (Rc 2 5))
  print (totalArea [Sq 1, Sq 2, Sq 3])
  print (totalArea [Rc 1 2, Rc 3 4])
