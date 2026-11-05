{-# LANGUAGE BangPatterns, LambdaCase, TupleSections, MultiWayIf #-}
-- FEAT-0118: syntactic extensions.
module Main where

classify :: Int -> String
classify n = if | n < 0 -> "neg" | n == 0 -> "zero" | n < 10 -> "small" | otherwise -> "big"

sumStrict :: [Int] -> Int
sumStrict = go 0
  where
    go !acc [] = acc
    go !acc (x : xs) = go (acc + x) xs

describe :: Maybe Int -> String
describe = \case
  Nothing -> "none"
  Just n
    | even n -> "even " ++ show n
    | otherwise -> "odd " ++ show n

main :: IO ()
main = do
  mapM_ (putStrLn . \case { 0 -> "zero"; n | even n -> "even" | otherwise -> "odd" }) [0, 1, 2 :: Int]
  print (map (,True) [1, 2 :: Int], map ("x",) "ab")
  print (map (,,'c') [1 :: Int] <*> [2 :: Int])
  print (map classify [-5, 0, 3, 42])
  print (sumStrict [1 .. 100000])
  mapM_ (putStrLn . describe) [Nothing, Just 4, Just 7]
  let !forced = length (replicate 3 'x')
  print forced
