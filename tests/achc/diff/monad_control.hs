-- Control.Monad: foldM, when, unless, zipWithM_, replicateM, forM_, filterM
import Control.Monad

safeSub :: Int -> Int -> Maybe Int
safeSub acc x
  | x > acc = Nothing
  | otherwise = Just (acc - x)

main :: IO ()
main = do
  print (foldM safeSub 100 [10, 20, 30])
  print (foldM safeSub 100 [10, 200, 30])
  print (foldM (\a x -> if x == 0 then Left "zero" else Right (a `div` x)) 1000 [2, 5, 10])
  print (foldM (\a x -> if x == 0 then Left "zero" else Right (a `div` x)) 1000 [2, 0, 10])
  zipWithM_ (\i s -> putStrLn (show i ++ ": " ++ s)) [1 :: Int ..] ["alpha", "beta", "gamma"]
  print (replicateM 2 "ab")
  print (length (replicateM 3 [0, 1 :: Int]))
  print (replicateM 2 (Just 'x'))
  when (length "abc" == 3) $ putStrLn "when: yes"
  unless (length "abc" == 3) $ putStrLn "unless: not printed"
  unless False $ putStrLn "unless: yes"
  print (filterM (const [True, False]) [1, 2, 3 :: Int])
  print (zipWithM (\a b -> if b /= 0 then Just (a `div` b) else Nothing) [10, 20, 30] [1, 2, 5 :: Int])
  print (join [[1, 2], [3 :: Int]])
  print (liftM2 (+) (Just 3) (Just (4 :: Int)))
  print (msum [Nothing, Just 1, Just (2 :: Int)])
  forM_ [1 .. 3 :: Int] $ \i -> when (odd i) $ print i
  print =<< foldM (\a x -> do { putStrLn ("add " ++ show x); return (a + x) }) 0 [1, 2, 3 :: Int]
