-- composition pipelines, point-free style, flip/curry/uncurry, either/maybe
import Data.Char (toUpper, isAlpha, ord)
import Data.Maybe (fromMaybe)

compose :: [a -> a] -> a -> a
compose = foldr (.) id

countVowels :: String -> Int
countVowels = length . filter (`elem` "aeiou")

shout :: String -> String
shout = map toUpper . filter isAlpha

sumSquaresOdd :: [Int] -> Int
sumSquaresOdd = sum . map (^ 2) . filter odd

twice :: (a -> a) -> a -> a
twice f = f . f

main :: IO ()
main = do
  print (countVowels "functional programming")
  putStrLn (shout "hello, world! 123")
  print (sumSquaresOdd [1 .. 10])
  print (compose [(+ 1), (* 2), subtract 3] 10)
  print (twice (twice (* 2)) 1)
  print (flip (-) 1 10)
  print (curry fst 'a' 'b')
  print (uncurry (+) (3, 4))
  print (map (uncurry max) [(1, 5), (7, 2), (3, 3)])
  print (zipWith ($) [(+ 1), (* 2), negate] [10, 20, 30])
  print (map (either length negate) [Left "abc", Right 5, Left ""])
  print (map (maybe 0 (+ 1)) [Just 1, Nothing, Just 10])
  print (fromMaybe 0 (lookup 'b' (zip "abc" [1 ..])))
  print ((sum . map ord) "abc")
  print (foldr (\x k acc -> k (acc * 10 + x)) id [1, 2, 3] 0)
  print (($ 3) <$> [(+ 1), (* 2)])
  print (until (> 1000) (* 2) 1)
