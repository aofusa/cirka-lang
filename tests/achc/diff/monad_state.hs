-- hand-written State monad with Functor/Applicative/Monad instances
newtype State s a = State { runState :: s -> (a, s) }

instance Functor (State s) where
  fmap f (State g) = State $ \s -> let (a, s') = g s in (f a, s')

instance Applicative (State s) where
  pure a = State $ \s -> (a, s)
  State f <*> State g = State $ \s ->
    let (h, s1) = f s
        (a, s2) = g s1
    in (h a, s2)

instance Monad (State s) where
  return = pure
  State g >>= f = State $ \s -> let (a, s') = g s in runState (f a) s'

get :: State s s
get = State $ \s -> (s, s)

put :: s -> State s ()
put s = State $ \_ -> ((), s)

modify :: (s -> s) -> State s ()
modify f = State $ \s -> ((), f s)

evalState :: State s a -> s -> a
evalState m s = fst (runState m s)

execState :: State s a -> s -> s
execState m s = snd (runState m s)

counter :: State Int Int
counter = do
  n <- get
  put (n + 1)
  return n

label :: [String] -> State Int [(Int, String)]
label = mapM (\x -> do { n <- counter; return (n, x) })

fibS :: Int -> State (Integer, Integer) Integer
fibS 0 = fmap fst get
fibS k = do
  (a, b) <- get
  put (b, a + b)
  fibS (k - 1)

sumList :: [Int] -> State Int ()
sumList = mapM_ (\x -> modify (+ x))

main :: IO ()
main = do
  print (runState counter 5)
  print (runState (counter >> counter >> counter) 0)
  print (evalState (label ["a", "b", "c", "d"]) 10)
  print (evalState (fibS 20) (0, 1))
  print (execState (sumList [1 .. 100]) 0)
  print (runState (fmap (* 2) counter) 21)
  print (evalState ((,) <$> counter <*> counter) 7)
  print (execState (mapM_ (\x -> modify (x :)) "hello") "")
  print (evalState (sequence (replicate 5 counter)) 100)
