-- nested forM_ loops producing a multiplication table and triangle
import Control.Monad
import Data.IORef
import System.IO

pad :: Int -> String -> String
pad n s = replicate (n - length s) ' ' ++ s

main :: IO ()
main = do
  forM_ [1 .. 5 :: Int] $ \i -> do
    forM_ [1 .. 5 :: Int] $ \j ->
      putStr (pad 4 (show (i * j)))
    putStrLn ""
  forM_ [1 .. 5 :: Int] $ \i ->
    putStrLn (replicate i '*')
  hPutStrLn stderr "stderr line"
  pairs <- newIORef (0 :: Int)
  forM_ [1 .. 10 :: Int] $ \i ->
    forM_ [i .. 10] $ \j ->
      when (i + j == 10) $ modifyIORef pairs (+ 1)
  n <- readIORef pairs
  putStrLn ("pairs summing to 10: " ++ show n)
  forM_ (zip [1 :: Int ..] "abc") $ \(i, c) ->
    putStrLn (show i ++ " -> " ++ [c])
  mapM_ (putStrLn . unwords . map show) [[i * j | j <- [1 .. i]] | i <- [1 .. 4 :: Int]]
