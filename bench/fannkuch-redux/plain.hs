-- fannkuch-redux (Benchmarks Game), single-threaded plain Haskell with
-- lists: aCHC and the GHC -O2 baseline. The permutations come in the order
-- of the counting algorithm of rust/main.rs (same checksum).
module Main where

import System.Environment (getArgs)

-- Number of prefix reversals until the first element is 0.
flips :: [Int] -> Int
flips = go 0
  where
    go !f p@(k : _)
      | k == 0 = f
      | otherwise = let (a, b) = splitAt (k + 1) p in go (f + 1) (reverse a ++ b)
    go f [] = f

-- count[i] = i + 1 for i = r - 1 down to 1.
fill :: Int -> [Int] -> [Int]
fill r count = [if i >= 1 && i < r then i + 1 else c | (i, c) <- zip [0 ..] count]

-- The next permutation (rotating the first r + 1 elements) and the new r.
next :: Int -> Int -> [Int] -> [Int] -> Maybe (Int, [Int], [Int])
next n r perm1 count
  | r == n = Nothing
  | otherwise =
      let (front, back) = splitAt (r + 1) perm1
          perm1' = tail front ++ [head front] ++ back
          c = count !! r - 1
          count' = take r count ++ [c] ++ drop (r + 1) count
       in if c > 0 then Just (r, perm1', count') else next n (r + 1) perm1' count'

fannkuch :: Int -> (Int, Int)
fannkuch n = go n [0 .. n - 1] (replicate n 0) 0 0 0
  where
    go r perm1 count !pc !cs !mf =
      let count' = forceList (fill r count)
          f = flips perm1
          cs' = if even pc then cs + f else cs - f
          mf' = max mf f
       in case next n 1 perm1 count' of
            Nothing -> (cs', mf')
            Just (r', p', c') -> go r' (forceList p') c' (pc + 1) cs' mf'

-- Evaluates the elements (the lists are rebuilt every permutation; left
-- lazy, the chains of `take`/`zip` thunks would grow without bound).
forceList :: [Int] -> [Int]
forceList xs = go xs `seq` xs
  where
    go [] = ()
    go (y : ys) = y `seq` go ys

main :: IO ()
main = do
  args <- getArgs
  let n = case args of { (s : _) -> read s; [] -> 7 }
      (checksum, f) = fannkuch n
  putStrLn (show checksum ++ "\nPfannkuchen(" ++ show n ++ ") = " ++ show f)
