-- prints some lines then exits with ExitFailure 3
import System.Exit
import System.IO

main :: IO ()
main = do
  putStrLn "starting"
  mapM_ (putStrLn . ("item " ++) . show) [1 .. 4 :: Int]
  hPutStrLn stderr "about to fail"
  putStrLn "exiting with 3"
  exitWith (ExitFailure 3)
  putStrLn "never printed"
