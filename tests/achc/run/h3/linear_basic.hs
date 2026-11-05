{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE LambdaCase #-}
-- LinearTypes without linear-base: linear arrows, constructors as linear
-- functions, case on linear values, and multiplicity polymorphism.
module Main where

data Pair a b = Pair a b

swapPair :: Pair a b %1 -> Pair b a
swapPair (Pair a b) = Pair b a

swapT :: (a, b) %1 -> (b, a)
swapT (x, y) = (y, x)

-- GHC 9.4 needs an eta-expansion to use a linear function as an
-- unrestricted one (aCHC also accepts `applyTwice linId`).
applyTwice :: (a -> a) -> a -> a
applyTwice f x = f (f x)

linId :: a %1 -> a
linId x = x

-- Multiplicity polymorphism.
app :: (a %m -> b) -> a %m -> b
app f x = f x

-- Case: every branch consumes the scrutinee's fields.
choose :: Bool -> (a, a) %1 -> Either (a, a) (a, a)
choose b p = case p of
  (x, y) -> if b then Left (x, y) else Right (y, x)

-- Linear lambdas and \case.
mapPair :: (a %1 -> c) -> (b %1 -> d) -> (a, b) %1 -> (c, d)
mapPair f g = \case (a, b) -> (f a, g b)

data Stream = Cons Int Stream | Nil

toList :: Stream %1 -> [Int]
toList Nil = []
toList (Cons n rest) = n : toList rest

showPair :: (Show a, Show b) => Pair a b -> String
showPair (Pair a b) = "Pair " ++ show a ++ " " ++ show b

main :: IO ()
main = do
  putStrLn (showPair (swapPair (Pair (1 :: Int) "one")))
  print (swapT (True, 'x'))
  print (applyTwice (\x -> linId x) (5 :: Int))
  print (app linId 'q', app negate (3 :: Int))
  print (choose True (1 :: Int, 2), choose False (1 :: Int, 2))
  print (mapPair Just (\x -> [x]) (1 :: Int, 2.5 :: Double))
  print (sum (toList (Cons 1 (Cons 2 (Cons 3 Nil)))))
