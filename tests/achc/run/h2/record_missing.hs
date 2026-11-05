data T = A { f :: Int, g :: Int } | B { f :: Int } deriving Show
main :: IO ()
main = do
  print (f (B 1), (A 1 2) { f = 9 })
  print (g (A { f = 1 }))
  print (g (B 1))
