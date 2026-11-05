-- IORef, whole-file IO, stdin to its end, stderr and hFlush (FEAT-0113).
import Data.IORef
import System.IO
import Control.Monad

counter :: IORef Int -> IO Int
counter r = do
  modifyIORef' r (+ 1)
  readIORef r

main :: IO ()
main = do
  hSetBuffering stdout NoBuffering
  r <- newIORef (0 :: Int)
  forM_ [1 .. 10] $ \i -> modifyIORef r (+ i)
  readIORef r >>= print
  xs <- newIORef ""
  mapM_ (\c -> modifyIORef' xs (c :)) "abc"
  readIORef xs >>= putStrLn
  n <- atomicModifyIORef' r (\x -> (x * 2, x + 1))
  print n
  c <- newIORef 0
  ks <- replicateM 3 (counter c)
  print (ks, n)
  pairs <- newIORef [(1 :: Int, "one")]
  modifyIORef pairs ((2, "two") :)
  readIORef pairs >>= print
  let path = "/tmp/achc-run-h2-io.txt"
  writeFile path "hello\n"
  appendFile path (unlines (map show [1 :: Int, 2, 3]))
  s <- readFile path
  putStr s
  print (length (lines s))
  hPutStrLn stderr "a line on stderr"
  hFlush stdout
  let loop acc = do
        eof <- isEOF
        if eof then return (reverse acc) else getLine >>= \l -> loop (l : acc)
  ls <- loop []
  print ls
