-- deriving (Eq, Ord, Show, Read, Enum, Bounded): output must match GHC.
module Main where

data Color = Red | Green | Blue
  deriving (Eq, Ord, Show, Read, Enum, Bounded)

data Shape = Circle Double | Rect { width :: Int, height :: Int } | Unit
  deriving (Eq, Ord, Show, Read)

data Tree a = Leaf | Node (Tree a) a (Tree a)
  deriving (Eq, Ord, Show, Read)

data Op = Int :+ Int | Neg Op
  deriving (Eq, Show, Read)

infixl 6 :+

newtype Wrap = Wrap { unwrap :: Maybe Int }
  deriving (Eq, Ord, Show, Read)

data P = P Int (Maybe Color) [Double] (Char, String)
  deriving (Eq, Ord, Show, Read)

main :: IO ()
main = do
  print [minBound .. maxBound :: Color]
  print (succ Red, pred Blue, fromEnum Blue, toEnum 1 :: Color, [Red ..])
  print (Red < Blue, compare Green Green, maximum [Green, Red, Blue])
  print (read "Blue" :: Color, read " ( Green ) " :: Color)
  print [Circle 1.5, Rect 2 3, Unit, Circle (-2)]
  print (Rect { width = -1, height = 4 })
  print (read "Rect {width = 5, height = 6}" :: Shape, read "Circle 2.0" :: Shape)
  print (Circle 1 < Rect 0 0, Rect 1 2 < Rect 1 3, Unit > Circle 9)
  let t = Node (Node Leaf 1 Leaf) 2 (Node Leaf (-3) Leaf) :: Tree Int
  print t
  print (read (show t) == t, t == Leaf, compare t Leaf)
  print (3 :+ 4, Neg (1 :+ (-2)), Neg (Neg (0 :+ 0)))
  print (read "Neg (1 :+ 2)" :: Op)
  print (Wrap (Just 3), Wrap Nothing < Wrap (Just 0))
  print (read "Wrap {unwrap = Just (-4)}" :: Wrap)
  let p = P (-1) (Just Red) [1.5, -0.25] ('\'', "q\"uote\n")
  print p
  print (read (show p) == p)
  print (showsPrec 11 (Just (-5 :: Int)) "", show (Just (Left (3, 'x') :: Either (Int, Char) Bool)))
  print (read "[(1,\"a\"),(2,\"b\")]" :: [(Int, String)])
  print (read "  ( 7 , -8 ) " :: (Int, Int), read "-3.5e-2" :: Double, read "[True,False]" :: [Bool])
