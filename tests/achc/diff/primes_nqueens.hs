-- list-based prime sieve and n-queens counting
sieve :: [Int] -> [Int]
sieve [] = []
sieve (p:xs) = p : sieve [x | x <- xs, x `mod` p /= 0]

isPrime :: Int -> Bool
isPrime n = n > 1 && all (\d -> n `mod` d /= 0) (takeWhile (\d -> d * d <= n) [2 ..])

queens :: Int -> [[Int]]
queens n = go n
  where
    go 0 = [[]]
    go k = [q : qs | qs <- go (k - 1), q <- [1 .. n], safe q qs]
    safe q qs = and [q /= c && abs (q - c) /= d | (d, c) <- zip [1 ..] qs]

main :: IO ()
main = do
  print (sieve [2 .. 60])
  print (length (sieve [2 .. 2000]))
  print (filter isPrime [90 .. 130])
  print (takeWhile (< 60) (filter isPrime [1 ..]))
  mapM_ (\n -> putStrLn (show n ++ " queens: " ++ show (length (queens n)))) [1 .. 7]
  print (head (queens 6))
  print (queens 4)
