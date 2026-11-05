-- CSV-like splitting on commas, trimming, numeric column sums, justification
module Main where

import Data.Char (isSpace, toUpper)
import Data.List (dropWhileEnd, intercalate)
import Text.Read (readMaybe)

splitOn :: Char -> String -> [String]
splitOn c s = case break (== c) s of
  (a, []) -> [a]
  (a, _ : rest) -> a : splitOn c rest

trim :: String -> String
trim = dropWhileEnd isSpace . dropWhile isSpace

csv :: String
csv = "name, age, city\nalice, 30, paris\nbob,25,  berlin \ncarol, , rome\n\ndave, 41,\n"

left, right, center :: Int -> String -> String
left w s = s ++ replicate (w - length s) ' '
right w s = replicate (w - length s) ' ' ++ s
center w s = replicate l ' ' ++ s ++ replicate (pad - l) ' '
  where pad = max 0 (w - length s)
        l = pad `div` 2

main :: IO ()
main = do
  let rows = map (map trim . splitOn ',') (filter (not . null) (lines csv))
  mapM_ print rows
  print (map length rows)
  let ages = [readMaybe a :: Maybe Int | (_ : a : _) <- tail rows]
  print ages
  print (sum [a | Just a <- ages])
  mapM_ (\r -> putStrLn ("|" ++ intercalate "|" [center 8 c | c <- r] ++ "|")) rows
  mapM_ (\r -> putStrLn ("|" ++ intercalate "|" [left 8 c | c <- r] ++ "|")) rows
  mapM_ (\r -> putStrLn ("|" ++ intercalate "|" [right 8 c | c <- r] ++ "|")) rows
  print (splitOn ',' "")
  print (splitOn ',' ",,")
  print (splitOn ';' "a;b;;c")
  putStrLn (map toUpper (intercalate "-" (map (take 3) (head rows))))
  print (unwords (words "  many   spaces here  "))
  print (lines "a\n\nb", unlines ["x", "y"])
