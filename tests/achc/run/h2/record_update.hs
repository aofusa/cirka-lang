data T = A { f :: Int, g :: Int } | B { f :: Int } deriving Show
main :: IO ()
main = print ((B 1) { g = 3 })
