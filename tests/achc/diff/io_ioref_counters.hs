-- IORef counters and accumulators with mapM_ over ranges and stderr output
import Data.IORef
import System.IO
import Control.Monad

main :: IO ()
main = do
  hSetBuffering stdout LineBuffering
  c <- newIORef (0 :: Int)
  mapM_ (\i -> modifyIORef c (+ i)) [1 .. 100]
  readIORef c >>= print
  hPutStrLn stderr "this goes to stderr"
  acc <- newIORef []
  forM_ [1 .. 5 :: Int] $ \i -> modifyIORef acc (i * i :)
  xs <- readIORef acc
  print xs
  putStrLn ("reversed: " ++ show (reverse xs))
  cnt <- newIORef (0 :: Int)
  let tick = atomicModifyIORef cnt (\n -> (n + 1, n))
  a <- tick
  b <- tick
  d <- tick
  print (a, b, d)
  total <- newIORef (0 :: Int)
  forM_ [1 .. 20 :: Int] $ \i ->
    when (i `mod` 3 == 0) $ do
      modifyIORef' total (+ i)
      v <- readIORef total
      putStrLn ("multiple of 3: " ++ show i ++ " running total " ++ show v)
  hPutStrLn stderr "done"
  writeIORef c 42
  v <- readIORef c
  putStrLn ("final: " ++ show v)
  hFlush stdout
