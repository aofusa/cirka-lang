-- show/read round trips and readMaybe
import Text.Read (readMaybe)

data Color = Red | Green | Blue deriving (Show, Read, Eq)
data P = P Int String deriving (Show, Read, Eq)

main :: IO ()
main = do
  print (read "42" :: Int)
  print (read " -7 " :: Int)
  print (read "[1,2,3]" :: [Int])
  print (read "(1,\"hi\")" :: (Int, String))
  print (readMaybe "12" :: Maybe Int)
  print (readMaybe "12x" :: Maybe Int)
  print (readMaybe "" :: Maybe Int)
  print (readMaybe "Blue" :: Maybe Color)
  print (readMaybe "Purple" :: Maybe Color)
  print (read (show (P (-3) "a\"b")) == P (-3) "a\"b")
  print (read "P 5 \"x\"" :: P)
  print (read (show [Just (-1), Nothing, Just (2 :: Int)]) :: [Maybe Int])
  print (read "3.5" :: Double)
  print (show (Just (-3 :: Int)), show [Left 1, Right 'c' :: Either Int Char])
