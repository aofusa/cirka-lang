-- Shape class with several instances, default method, and a sum type for heterogeneous lists
module Main where

class Shape a where
  area :: a -> Double
  perimeter :: a -> Double
  label :: a -> String
  describe :: a -> String
  describe x = label x ++ ": area=" ++ show (area x) ++ " perimeter=" ++ show (perimeter x)

data Rect = Rect Double Double
data Circle = Circle Double
data RightTri = RightTri Double Double

instance Shape Rect where
  area (Rect w h) = w * h
  perimeter (Rect w h) = 2 * (w + h)
  label _ = "rect"

instance Shape Circle where
  area (Circle r) = 3 * r * r
  perimeter (Circle r) = 6 * r
  label _ = "circle"
  describe c = "round " ++ label c ++ " of area " ++ show (area c)

instance Shape RightTri where
  area (RightTri a b) = a * b / 2
  perimeter (RightTri a b) = a + b + sqrt (a * a + b * b)
  label _ = "triangle"

data AnyShape = SR Rect | SC Circle | ST RightTri

instance Shape AnyShape where
  area (SR s) = area s
  area (SC s) = area s
  area (ST s) = area s
  perimeter (SR s) = perimeter s
  perimeter (SC s) = perimeter s
  perimeter (ST s) = perimeter s
  label (SR s) = label s
  label (SC s) = label s
  label (ST s) = label s
  describe (SR s) = describe s
  describe (SC s) = describe s
  describe (ST s) = describe s

shapes :: [AnyShape]
shapes = [SR (Rect 3 4), SC (Circle 2), ST (RightTri 3 4), ST (RightTri 5 12), SR (Rect 1.5 2)]

largest :: [AnyShape] -> AnyShape
largest = foldr1 (\a b -> if area a >= area b then a else b)

main :: IO ()
main = do
  mapM_ (putStrLn . describe) shapes
  print (sum (map area shapes))
  print (map perimeter shapes)
  putStrLn (label (largest shapes))
  print (length [() | s <- shapes, area s > 5])
  putStrLn (describe (Rect 2 2))
  putStrLn (describe (Circle 1))
  print (maximum (map perimeter shapes), minimum (map area shapes))
