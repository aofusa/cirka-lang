data T = A { f :: Int, g :: Int } | B { f :: Int } deriving Show
main :: IO ()
main = print (g (B 1))
