-- Mutually recursive functions (FEAT-0071): the optimizer keeps one function
-- of each group as a call and inlines the others. The program means the same:
-- `f` never forces its second argument on the way, and `isOdd` is also used
-- as a value.
module Main where

isEven, isOdd :: Int -> Bool
isEven 0 = True
isEven n = isOdd (n - 1)
isOdd 0 = False
isOdd n = isEven (n - 1)

f :: Int -> Int -> Int
f 0 _ = 0
f n x = g (n - 1) x

g :: Int -> Int -> Int
g 0 x = x
g n x = f (n - 1) x

a :: Int -> Int
a 0 = 1
a n = n + b (n - 1)

b :: Int -> Int
b n = c n * 2

c :: Int -> Int
c n = if n > 100 then 0 else a n

main :: IO ()
main = do
  print (map isEven [0, 1, 10, 11])
  print (f 4 (error "not forced"))
  print (g 1 7)
  print (g 0 7)
  print (a 5)
  print (length (filter isOdd [1 .. 7]))
