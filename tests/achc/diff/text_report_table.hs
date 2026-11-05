-- fixed-width report with padding, alignment, computed column widths and totals
module Main where

import Data.List (transpose, intercalate)

padR, padL :: Int -> String -> String
padR n s = s ++ replicate (n - length s) ' '
padL n s = replicate (n - length s) ' ' ++ s

money :: Int -> String
money c = show (c `div` 100) ++ "." ++ pad2 (c `mod` 100)
  where pad2 n = if n < 10 then '0' : show n else show n

items :: [(String, Int, Int)]
items =
  [ ("widget", 4, 250)
  , ("gadget", 10, 1999)
  , ("thingamajig", 1, 12005)
  , ("nut", 250, 5)
  , ("bolt", 120, 12)
  ]

row :: (String, Int, Int) -> String
row (n, q, p) =
  padR 12 n ++ " | " ++ padL 4 (show q) ++ " | " ++ padL 9 (money p) ++ " | " ++ padL 10 (money (q * p))

table :: [[String]] -> [String]
table rows = map fmt rows
  where
    widths = map (maximum . map length) (transpose rows)
    fmt cells = intercalate " | " [padR w c | (w, c) <- zip widths cells]

main :: IO ()
main = do
  putStrLn (padR 12 "item" ++ " | " ++ padL 4 "qty" ++ " | " ++ padL 9 "price" ++ " | " ++ padL 10 "total")
  putStrLn (replicate 45 '-')
  mapM_ (putStrLn . row) items
  putStrLn (replicate 45 '=')
  let total = sum [q * p | (_, q, p) <- items]
  putStrLn (padR 12 "TOTAL" ++ "   " ++ padL 4 (show (sum [q | (_, q, _) <- items])) ++ "   " ++ padL 9 "" ++ "   " ++ padL 10 (money total))
  putStrLn ""
  mapM_ putStrLn (table [["name", "len", "upper"], ["a", "1", "A"], ["hello", "5", "HELLO"], ["xy", "2", "XY"]])
  putStrLn ""
  mapM_ (\(i, (n, _, _)) -> putStrLn (padL 3 (show i) ++ ". " ++ n)) (zip [1 :: Int ..] items)
  putStrLn ("[" ++ padL 6 "ab" ++ "][" ++ padR 6 "ab" ++ "][" ++ padL 1 "abc" ++ "]")
