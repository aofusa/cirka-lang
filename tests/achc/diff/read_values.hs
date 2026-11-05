-- read, reads and readMaybe on lists, tuples, Maybe, derived Read
module Main where

import Text.Read (readMaybe)

data Color = Red | Green | Blue deriving (Show, Read, Eq, Ord)

data P = P { px :: Int, py :: Int } deriving (Show, Read, Eq)

data Shape = Circle Int | Rect Int Int deriving (Show, Read, Eq)

main :: IO ()
main = do
  print (read "[1,2,3]" :: [Int])
  print (read " [ 1 , -2 , 3 ] " :: [Int])
  print (read "(1,\"a\")" :: (Int, String))
  print (read "(True,'x',2.5)" :: (Bool, Char, Double))
  print (read " Just (-3) " :: Maybe Int)
  print (read "Nothing" :: Maybe Int)
  print (read "[Just 1,Nothing,Just 3]" :: [Maybe Int])
  print (read "-5" :: Int)
  print (read "3.5e2" :: Double)
  print (read "[Red,Green,Blue]" :: [Color])
  print (read "P {px = 1, py = -2}" :: P)
  print (read "[Circle 3,Rect 4 5]" :: [Shape])
  print (read "Left 4" :: Either Int Bool)
  print (reads "12abc" :: [(Int, String)])
  print (reads "  7 rest" :: [(Int, String)])
  print (reads "abc" :: [(Int, String)])
  print (reads "Red Green" :: [(Color, String)])
  print (readMaybe "42" :: Maybe Int)
  print (readMaybe "4x2" :: Maybe Int)
  print (readMaybe "" :: Maybe Int)
  print (readMaybe "Purple" :: Maybe Color)
  print (readMaybe "[1,2" :: Maybe [Int])
  print (map readMaybe (words "1 two 3 4.5") :: [Maybe Int])
  print (read (show (Rect 2 (-3))) == Rect 2 (-3))
  print (read "\"hi\\nthere\"" :: String)
