-- greedy word wrapping at width 20 and vowel/consonant counting
module Main where

import Data.Char (isAlpha, toLower)
import Data.List (intercalate)

wrap :: Int -> [String] -> [String]
wrap _ [] = []
wrap w (x : xs) = go x xs
  where
    go cur [] = [cur]
    go cur (y : ys)
      | length cur + 1 + length y <= w = go (cur ++ " " ++ y) ys
      | otherwise = cur : go y ys

isVowel :: Char -> Bool
isVowel c = toLower c `elem` "aeiou"

counts :: String -> (Int, Int)
counts s = (length (filter isVowel letters), length (filter (not . isVowel) letters))
  where letters = filter isAlpha s

text :: String
text = "Haskell is a purely functional programming language with strong static typing, \
       \lazy evaluation and type inference. Extraordinarily long words like \
       \internationalization may exceed nothing."

main :: IO ()
main = do
  let ls = wrap 20 (words text)
  mapM_ (\l -> putStrLn ("|" ++ l ++ replicate (20 - length l) ' ' ++ "|")) ls
  print (length ls)
  print (map length ls)
  let (v, c) = counts text
  putStrLn ("vowels: " ++ show v)
  putStrLn ("consonants: " ++ show c)
  mapM_ (\w -> putStrLn (w ++ " " ++ show (counts w))) (take 6 (words text))
  print (maximum (map length (words text)))
  putStrLn (intercalate "/" (wrap 10 (words "a bb ccc dddd eeeee ffffff")))
  print (wrap 3 ["abcdef", "g", "hi"])
