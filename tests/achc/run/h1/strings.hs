-- String output and simple text processing.
import Data.Char (toUpper, isDigit, ord, chr)

shout :: String -> String
shout s = map toUpper s ++ "!"

caesar :: Int -> String -> String
caesar k = map shift
  where
    shift c
      | ord c >= 97 && ord c <= 122 = chr ((ord c - 97 + k) `mod` 26 + 97)
      | otherwise = c

digits :: String -> Int
digits = length . filter isDigit

main :: IO ()
main = do
  putStrLn "Hello, World!"
  putStrLn (shout "lazy evaluation without gc")
  putStr (unlines (map (\n -> show n ++ " squared is " ++ show (n * n)) [1 .. 3]))
  putStrLn (unwords (reverse (words "one two  three")))
  putStrLn (caesar 3 "hello world")
  print (digits "a1b2c3d45")
  mapM_ putStrLn (lines "first\nsecond\nthird")
  putStrLn ("unicode: " ++ "λ→ü")
  print (length "λ→ü")
