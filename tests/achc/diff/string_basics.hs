-- words/unwords/lines/unlines and Data.Char functions
import Data.Char

main :: IO ()
main = do
  let s = "  hello   world  from\thaskell\n again "
  print (words s)
  putStrLn (unwords (words s))
  print (lines "a\nb\n\nc")
  print (unlines ["x", "y", "z"])
  print (map toUpper "Hello, World 42")
  print (map ord "Az09")
  print (map chr [72, 105, 33])
  print (filter isDigit "a1b22c333")
  print (map digitToInt "1aF")
  print (map intToDigit [0, 9, 10, 15])
  print (isAlpha 'x', isSpace '\t', isUpper 'a', isLower 'a', isAlphaNum '_', isPunctuation '!')
  print (span isAlpha "abc123def")
  print (show "quote\"d\ttab\n")
  print (reverse (words "one two three"))
