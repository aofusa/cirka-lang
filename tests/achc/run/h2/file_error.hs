-- Reading a missing file ends the program like GHC's uncaught IOError.
main :: IO ()
main = do
  putStrLn "reading"
  s <- readFile "/nonexistent/achc/file.txt"
  putStr s
