-- div/mod/quot/rem on negatives and rounding functions
main :: IO ()
main = do
  let ps = [(7, 2), (-7, 2), (7, -2), (-7, -2)] :: [(Int, Int)]
  mapM_ (\(a, b) -> print (a, b, a `div` b, a `mod` b, a `quot` b, a `rem` b)) ps
  print (map (\(a, b) -> divMod a b) ps)
  print (map (\(a, b) -> quotRem a b) ps)
  print (map round [0.5, 1.5, 2.5, -0.5, -1.5, 2.6, -2.6 :: Double] :: [Int])
  print (map truncate [2.7, -2.7 :: Double] :: [Int])
  print (map floor [2.7, -2.7 :: Double] :: [Int])
  print (map ceiling [2.2, -2.2 :: Double] :: [Int])
  print (fromIntegral (3 :: Int) / 2 :: Double)
  print (fromIntegral (-3 :: Int) * 1.5 :: Double)
  print (toInteger (minBound :: Int))
  print ((-5) `mod` 3 :: Integer, (-5) `rem` 3 :: Integer, 5 `mod` (-3) :: Integer)
  print (abs (-4 :: Int), signum (-4 :: Int), signum (0 :: Int), negate 5 :: Int)
  print (2 ^ 10 :: Int, 10 ^ 0 :: Int)
