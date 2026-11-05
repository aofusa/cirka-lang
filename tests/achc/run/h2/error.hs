-- `error`: GHC's message with the call site, exit code 1, output so far kept.
main :: IO ()
main = do
  putStrLn "before"
  let xs = [1, 2, 3] :: [Int]
  print (sum xs)
  if sum xs > 5 then error ("too big: " ++ show (sum xs)) else putStrLn "small"
  putStrLn "after"
