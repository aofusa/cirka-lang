-- Maybe and Either monads with do-notation, mapM, sequence
import Data.Char (isDigit)

safeDiv :: Int -> Int -> Maybe Int
safeDiv _ 0 = Nothing
safeDiv a b = Just (a `div` b)

calc :: Int -> Int -> Int -> Maybe Int
calc a b c = do
  x <- safeDiv a b
  y <- safeDiv x c
  return (x + y)

parseDigit :: Char -> Either String Int
parseDigit c
  | isDigit c = Right (fromEnum c - fromEnum '0')
  | otherwise = Left ("bad digit: " ++ [c])

parseNum :: String -> Either String Int
parseNum s = do
  ds <- mapM parseDigit s
  return (foldl (\a d -> a * 10 + d) 0 ds)

addParsed :: String -> String -> Either String Int
addParsed a b = do
  x <- parseNum a
  y <- parseNum b
  if x + y > 1000 then Left "too big" else Right (x + y)

main :: IO ()
main = do
  print (calc 100 5 2)
  print (calc 100 0 2)
  print (calc 100 5 0)
  print (mapM (safeDiv 100) [1, 2, 5])
  print (mapM (safeDiv 100) [1, 0, 5])
  print (sequence [Just 1, Just 2, Just (3 :: Int)])
  print (sequence [Just 1, Nothing, Just (3 :: Int)])
  print (parseNum "12345")
  print (parseNum "12x45")
  print (addParsed "12" "30")
  print (addParsed "999" "2")
  print (addParsed "1a" "2")
  print (sequence [Right 1, Left "e1", Left "e2" :: Either String Int])
  print (either length (* 2) (parseNum "21"))
