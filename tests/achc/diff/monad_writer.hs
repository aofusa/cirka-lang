-- hand-written Writer-like monad logging computation steps
newtype Writer w a = Writer { runWriter :: (a, w) }

instance Functor (Writer w) where
  fmap f (Writer (a, w)) = Writer (f a, w)

instance Monoid w => Applicative (Writer w) where
  pure a = Writer (a, mempty)
  Writer (f, w1) <*> Writer (a, w2) = Writer (f a, w1 <> w2)

instance Monoid w => Monad (Writer w) where
  return = pure
  Writer (a, w) >>= f = let Writer (b, w') = f a in Writer (b, w <> w')

tell :: w -> Writer w ()
tell w = Writer ((), w)

gcdW :: Int -> Int -> Writer [String] Int
gcdW a 0 = do
  tell ["done: " ++ show a]
  return a
gcdW a b = do
  tell [show a ++ " mod " ++ show b ++ " = " ++ show (a `mod` b)]
  gcdW b (a `mod` b)

factW :: Int -> Writer [String] Integer
factW 0 = tell ["fact 0 = 1"] >> return 1
factW n = do
  r <- factW (n - 1)
  let v = fromIntegral n * r
  tell ["fact " ++ show n ++ " = " ++ show v]
  return v

countW :: [Int] -> Writer [Int] Int
countW xs = do
  mapM_ (\x -> if even x then tell [x] else return ()) xs
  return (length xs)

main :: IO ()
main = do
  let (g, logs) = runWriter (gcdW 48 18)
  print g
  mapM_ putStrLn logs
  let (f, fl) = runWriter (factW 5)
  print f
  mapM_ putStrLn fl
  print (runWriter (countW [1 .. 10]))
  print (runWriter (fmap (+ 1) (tell "ab" >> return (1 :: Int))))
  print (snd (runWriter (mapM_ (\x -> tell [show x]) [1, 2, 3 :: Int])))
