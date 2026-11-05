-- caesar cipher, palindromes and run-length encoding
import Data.Char
import Data.List (group)

shift :: Int -> Char -> Char
shift n c
  | isLower c = rot 'a'
  | isUpper c = rot 'A'
  | otherwise = c
  where rot base = chr (ord base + (ord c - ord base + n) `mod` 26)

caesar :: Int -> String -> String
caesar n = map (shift n)

isPalin :: String -> Bool
isPalin s = cleaned == reverse cleaned
  where cleaned = map toLower (filter isAlpha s)

rle :: String -> [(Char, Int)]
rle = map (\g -> (head g, length g)) . group

unrle :: [(Char, Int)] -> String
unrle = concatMap (\(c, n) -> replicate n c)

rleStr :: String -> String
rleStr = concatMap (\(c, n) -> c : show n) . rle

main :: IO ()
main = do
  putStrLn (caesar 3 "Hello, World!")
  putStrLn (caesar (-3) (caesar 3 "Hello, World!"))
  putStrLn (caesar 13 (caesar 13 "Round Trip"))
  print (map isPalin ["A man, a plan, a canal: Panama", "hello", "", "Madam"])
  print (rle "aaabccddddd")
  putStrLn (rleStr "aaabccddddd")
  print (unrle (rle "mississippi") == "mississippi")
  print (rle "")
