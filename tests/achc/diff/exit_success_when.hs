-- exitSuccess called from inside a when, stopping a loop early
import System.Exit
import Control.Monad

main :: IO ()
main = do
  putStrLn "begin"
  forM_ [1 .. 10 :: Int] $ \i -> do
    putStrLn ("i = " ++ show i)
    when (i * i > 20) $ do
      putStrLn "limit reached"
      exitSuccess
  putStrLn "never printed"
