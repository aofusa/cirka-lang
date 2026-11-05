{-# LANGUAGE BangPatterns #-}
-- spectral-norm n=50, mandelbrot membership counts, word frequency with sort/group
module Main where

import Data.List (sort, group, sortBy)
import Data.Char (toLower, isAlpha)

n :: Int
n = 50

a :: Int -> Int -> Double
a i j = 1 / fromIntegral ((i + j) * (i + j + 1) `div` 2 + i + 1)

mulAv :: [Double] -> [Double]
mulAv v = [sum [a i j * vj | (j, vj) <- zip [0 ..] v] | i <- [0 .. n - 1]]

mulAtv :: [Double] -> [Double]
mulAtv v = [sum [a j i * vj | (j, vj) <- zip [0 ..] v] | i <- [0 .. n - 1]]

mulAtAv :: [Double] -> [Double]
mulAtAv = mulAtv . mulAv

spectral :: Double
spectral = sqrt (vBv / vv)
  where
    u0 = replicate n 1.0
    loop :: Int -> [Double] -> [Double] -> ([Double], [Double])
    loop 0 u v = (u, v)
    loop k u _ = let v' = mulAtAv u
                     u' = mulAtAv v'
                 in loop (k - 1) u' v'
    (u, v) = loop 10 u0 u0
    vBv = sum (zipWith (*) u v)
    vv = sum (zipWith (*) v v)

inSet :: Double -> Double -> Bool
inSet cr ci = go 0 0 0
  where
    go :: Double -> Double -> Int -> Bool
    go !x !y !i
      | i >= 50 = True
      | x * x + y * y > 4 = False
      | otherwise = go (x * x - y * y + cr) (2 * x * y + ci) (i + 1)

rowOf :: Int -> String
rowOf r = [ if inSet (-2 + 3 * fromIntegral c / 40) (-1 + 2 * fromIntegral r / 20) then '#' else '.'
          | c <- [0 .. 39 :: Int] ]

text :: String
text = "the quick brown fox jumps over the lazy dog the dog barks and the fox runs away \
       \while the quick cat watches the dog and the fox"

cmp :: (String, Int) -> (String, Int) -> Ordering
cmp (w1, c1) (w2, c2) = case compare c2 c1 of
  EQ -> compare w1 w2
  o -> o

main :: IO ()
main = do
  let x = spectral
  print (fromIntegral (round (x * 1e9) :: Integer) / 1e9 :: Double)
  let rows = map rowOf [0 .. 19]
  mapM_ putStrLn rows
  let counts = map (length . filter (== '#')) rows
  print counts
  putStrLn ("total in set: " ++ show (sum counts))
  let ws = words (map toLower (filter (\c -> isAlpha c || c == ' ') text))
      freq = sortBy cmp [(head g, length g) | g <- group (sort ws)]
  mapM_ (\(w, c) -> putStrLn (w ++ ": " ++ show c)) freq
  print (length ws)
