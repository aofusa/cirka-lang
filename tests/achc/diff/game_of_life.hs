-- Game of Life on a small grid using lists of lists
type Grid = [[Bool]]

fromStrs :: [String] -> Grid
fromStrs = map (map (== '#'))

render :: Grid -> [String]
render = map (map (\b -> if b then '#' else '.'))

cell :: Grid -> Int -> Int -> Bool
cell g r c
  | r < 0 || c < 0 || r >= length g || c >= length (head g) = False
  | otherwise = g !! r !! c

neighbors :: Grid -> Int -> Int -> Int
neighbors g r c = length [() | dr <- [-1, 0, 1], dc <- [-1, 0, 1], (dr, dc) /= (0, 0), cell g (r + dr) (c + dc)]

stepG :: Grid -> Grid
stepG g = [[rule (cell g r c) (neighbors g r c) | c <- [0 .. w - 1]] | r <- [0 .. h - 1]]
  where
    h = length g
    w = length (head g)
    rule True n = n == 2 || n == 3
    rule False n = n == 3

population :: Grid -> Int
population = length . filter id . concat

main :: IO ()
main = do
  let glider = fromStrs [".#....", "..#...", "###...", "......", "......", "......"]
      gens = take 5 (iterate stepG glider)
  mapM_ (\(i, g) -> do
           putStrLn ("gen " ++ show (i :: Int) ++ " pop=" ++ show (population g))
           mapM_ putStrLn (render g)) (zip [0 ..] gens)
  let blinker = fromStrs [".....", "..#..", "..#..", "..#..", "....."]
  print (render (stepG blinker))
  print (stepG (stepG blinker) == blinker)
