{-# LANGUAGE BangPatterns #-}
-- error forced with a bang pattern before printing; exit code 1
safeAvg :: [Int] -> Int
safeAvg [] = error "average of empty list"
safeAvg xs = sum xs `div` length xs

main :: IO ()
main = do
  putStrLn "line 1"
  putStrLn ("avg " ++ show (safeAvg [2, 4, 6]))
  putStrLn "line 3"
  putStrLn "line 4"
  putStrLn "line 5"
  let !x = safeAvg []
  putStrLn ("avg of empty " ++ show x)
  putStrLn "unreachable"
