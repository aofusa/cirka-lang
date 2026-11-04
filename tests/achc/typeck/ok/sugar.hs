{-# LANGUAGE LambdaCase, TupleSections, MultiWayIf #-}
module Main where

data Shape = Circle { radius :: Int } | Rect { width :: Int, height :: Int }

area = \case
  Circle r -> r + r
  Rect w h -> w + h

grow s = s { radius = 1 }

pairs = map (, True) [1, 2]

classify n = if | n == 0 -> "zero"
                | otherwise -> "other"
  where otherwise = True

firsts xs = [a | (a, _) <- xs]

sections = (map (+ 1) [1], map (1 -) [2], map (`sub` 1) [3])
  where sub = (-)

main = putStr (classify (area (Rect 1 2)))
