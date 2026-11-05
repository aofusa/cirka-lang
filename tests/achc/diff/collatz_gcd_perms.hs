-- Collatz, gcd/lcm, permutations and subsets
import Data.List (permutations, subsequences, sort, maximumBy)
import Data.Ord (comparing)

collatz :: Int -> Int
collatz 1 = 0
collatz n
  | even n = 1 + collatz (n `div` 2)
  | otherwise = 1 + collatz (3 * n + 1)

myGcd :: Int -> Int -> Int
myGcd a 0 = abs a
myGcd a b = myGcd b (a `mod` b)

perms :: [a] -> [[a]]
perms [] = [[]]
perms xs = [y : p | (y, rest) <- picks xs, p <- perms rest]
  where picks [] = []
        picks (z:zs) = (z, zs) : [(w, z : ws) | (w, ws) <- picks zs]

subsets :: [a] -> [[a]]
subsets [] = [[]]
subsets (x:xs) = let r = subsets xs in map (x :) r ++ r

main :: IO ()
main = do
  print (map collatz [1 .. 15])
  print (maximumBy (comparing collatz) [1 .. 1000])
  print (myGcd 48 18, gcd 48 18, lcm 4 6 :: Int)
  print (gcd (-12) 18 :: Int, lcm (-4) 6 :: Int, gcd 0 0 :: Int)
  print (perms [1, 2, 3 :: Int])
  print (sort (perms "abc") == sort (permutations "abc"))
  print (length (perms [1 .. 6 :: Int]))
  print (subsets "abc")
  print (sort (map sort (subsets [1, 2, 3 :: Int])) == sort (map sort (subsequences [1, 2, 3])))
  print (length (subsets [1 .. 10 :: Int]))
