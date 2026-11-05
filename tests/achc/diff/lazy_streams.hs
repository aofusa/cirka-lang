-- infinite lists with iterate, cycle, repeat, zip and take
main :: IO ()
main = do
  print (take 10 (iterate (*2) 1) :: [Int])
  print (take 7 (cycle [1,2,3 :: Int]))
  print (take 5 (repeat 'x'))
  print (take 5 (zip [1 :: Int ..] "abcdefg"))
  print (takeWhile (< 40) (map (^ 2) [1 :: Int ..]))
  print (take 6 (zipWith (+) [1 :: Int ..] (cycle [10, 20])))
  print (take 8 (filter even (iterate (\x -> 3 * x + 1) (1 :: Int))))
  print (head (dropWhile (< 1000) (iterate (* 3) (1 :: Int))))
  print (zip3 [1 :: Int ..] "abc" (cycle [True, False]))
  print (take 4 (scanl (+) 0 [1 :: Int ..]))
  print (lookup 5 (zip [1 :: Int ..] (words "a b c d e f g")))
