-- flags: --cycles=collect
-- The cycle collector (FEAT-0126) under load: about 3000 short-lived
-- cycles trigger many collections while a long-lived cycle (`keep`) and
-- references stay in use across them; nothing leaks at the end.
module Main where

import Data.IORef

newtype Node = Node (IORef [Node])

ring :: Int -> [Int]
ring n = let r = [n, n + 1, n + 2] ++ r in r

main :: IO ()
main = do
  let keep = 7 : 8 : keep :: [Int]
  acc <- newIORef (0 :: Int)
  mapM_
    ( \i -> do
        modifyIORef' acc (+ sum (take 5 (ring i)))
        r <- newIORef []
        writeIORef r [Node r]
        modifyIORef' acc (+ head (drop i keep))
    )
    [1 .. 1500]
  readIORef acc >>= print
  print (take 4 keep)
