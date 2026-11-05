-- where-bound lazy values that are only forced when needed
safeDiv :: Int -> Int -> Maybe Int
safeDiv a b
  | b == 0 = Nothing
  | otherwise = Just q
  where q = a `div` b

pick :: Bool -> Int -> Int
pick c n = if c then n else 0
  where _unused = error "forced" :: Int

choose :: Int -> String
choose n
  | n < 0 = "neg"
  | n < limit = "small"
  | otherwise = "big"
  where limit = expensive 10
        expensive k = sum [1 .. k]

firstBig :: [Int] -> Int
firstBig xs = head big
  where big = filter (> 50) (map (* 7) xs)

main :: IO ()
main = do
  print (safeDiv 10 2)
  print (safeDiv 1 0)
  print (pick True 7)
  print (pick False 7)
  mapM_ (putStrLn . choose) [-3, 4, 55, 56, 100]
  print (firstBig [1 ..])
  print (let ys = map (* 2) [1 :: Int ..] in ys !! 1000)
