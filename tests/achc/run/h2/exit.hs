-- exitWith: the exit code and flushed output; getProgName/getArgs.
import System.Exit
import System.Environment
import Control.Monad

main :: IO ()
main = do
  args <- getArgs
  print args
  name <- getProgName
  print (length name > 0)
  forM_ [1 :: Int ..] $ \i -> do
    print i
    when (i == 3) $ exitWith (ExitFailure 3)
