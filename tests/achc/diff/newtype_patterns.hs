-- opt-levels: 0 2
-- BUG-0008: matching a newtype constructor evaluates nothing (Haskell 2010
-- §4.2.3); only the patterns of its field decide what gets evaluated.
module Main where

newtype T = C { f :: String }

newtype N = N Int

newtype W a = W a

newtype Nest = Nest N

data P = P N Int

ignores :: N -> Int
ignores (N _) = 1

usedLate :: N -> Int -> Int
usedLate (N x) y = if y > 0 then y else x

clauses :: N -> Int
clauses (N 0) = 10
clauses (N 1) = 11
clauses (N n) = n + 100

twoColumns :: N -> Bool -> Int
twoColumns (N _) True = 20
twoColumns (N n) False = n

inData :: P -> Int
inData (P (N _) y) = y

main :: IO ()
main = do
  print $ case C "abc" of
    C {f = v} -> v
  print $ case undefined of
    C {} -> True
  print (case undefined of C _ -> 'a')
  print (case undefined of N _ -> (2 :: Int))
  print (case undefined of W _ -> (3 :: Int))
  print (case undefined of Nest (N _) -> (4 :: Int))
  print (ignores undefined)
  print (usedLate undefined 5)
  print (usedLate (N 6) 0)
  print (map clauses [N 0, N 1, N 2])
  print (twoColumns undefined True)
  print (twoColumns (N 7) False)
  print (inData (P undefined 8))
  print ((\(N _) -> 9 :: Int) undefined)
  print (case (undefined, 10 :: Int) of (N _, b) -> b)
  print (f ((undefined :: T) {f = "updated"}))
