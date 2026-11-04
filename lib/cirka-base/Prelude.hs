{-# LANGUAGE NoImplicitPrelude #-}
-- | The Prelude of aCHC H1 (docs/design/09-libraries.md §1).
--
-- Type classes come with H2 (FEAT-0110). Until then the numeric and
-- comparison operators work on `Int` only, and `show` / `print` take an
-- `Int`. Programs that only use these types behave as with GHC.
module Prelude
  ( module Prelude
  , Int, Char, Double, IO, Bool(..)
  ) where

import Cirka.Prim

infixr 9 .
infixl 9 !!
infixr 8 ^
infixl 7 *, `div`, `mod`, `quot`, `rem`
infixl 6 +, -
infixr 5 ++
infix 4 ==, /=, <, <=, >, >=, `elem`, `notElem`
infixr 3 &&
infixr 2 ||
infixl 1 >>=, >>
infixr 0 $, $!, `seq`

type String = [Char]

data Maybe a = Nothing | Just a
data Either a b = Left a | Right b
data Ordering = LT | EQ | GT

-- ---- functions ----

id :: a -> a
id x = x

const :: a -> b -> a
const x _ = x

(.) :: (b -> c) -> (a -> b) -> a -> c
(f . g) x = f (g x)

flip :: (a -> b -> c) -> b -> a -> c
flip f x y = f y x

($) :: (a -> b) -> a -> b
f $ x = f x

seq :: a -> b -> b
seq = primSeq

($!) :: (a -> b) -> a -> b
f $! x = x `seq` f x

until :: (a -> Bool) -> (a -> a) -> a -> a
until p f x = if p x then x else until p f (f x)

error :: String -> a
error = primError

undefined :: a
undefined = error "Prelude.undefined"

-- ---- Bool ----

(&&) :: Bool -> Bool -> Bool
True && x = x
False && _ = False

(||) :: Bool -> Bool -> Bool
True || _ = True
False || x = x

not :: Bool -> Bool
not True = False
not False = True

otherwise :: Bool
otherwise = True

-- ---- Int ----

(+), (-), (*), quot, rem, div, mod, max, min, gcd, lcm, (^), subtract :: Int -> Int -> Int
(+) = primIntAdd
(-) = primIntSub
(*) = primIntMul
quot = primIntQuot
rem = primIntRem
div x y =
  let q = quot x y
      r = rem x y
  in if (r < 0 && y > 0) || (r > 0 && y < 0) then q - 1 else q
mod x y =
  let r = rem x y
  in if (r < 0 && y > 0) || (r > 0 && y < 0) then r + y else r
max x y = if x <= y then y else x
min x y = if x <= y then x else y
gcd x y = gcd' (abs x) (abs y)
  where gcd' a 0 = a
        gcd' a b = gcd' b (rem a b)
lcm _ 0 = 0
lcm 0 _ = 0
lcm x y = abs (quot x (gcd x y) * y)
x ^ n
  | n < 0 = error "Negative exponent"
  | n == 0 = 1
  | even n = let h = x ^ quot n 2 in h * h
  | otherwise = x * x ^ (n - 1)
subtract x y = y - x

negate, abs, signum, succ, pred, fromIntegral, fromEnum, toEnum :: Int -> Int
negate = primIntNeg
abs x = if x < 0 then negate x else x
signum x
  | x < 0 = negate 1
  | x == 0 = 0
  | otherwise = 1
succ x = x + 1
pred x = x - 1
fromIntegral x = x
fromEnum x = x
toEnum x = x

(==), (/=), (<), (<=), (>), (>=) :: Int -> Int -> Bool
(==) = primIntEq
(/=) = primIntNe
(<) = primIntLt
(<=) = primIntLe
(>) = primIntGt
(>=) = primIntGe

compare :: Int -> Int -> Ordering
compare x y
  | x < y = LT
  | x == y = EQ
  | otherwise = GT

even, odd :: Int -> Bool
even n = rem n 2 == 0
odd n = rem n 2 /= 0

show :: Int -> String
show = primShowInt

-- ---- enumerations ----

enumFrom :: Int -> [Int]
enumFrom n = n : enumFrom (n + 1)

enumFromThen :: Int -> Int -> [Int]
enumFromThen a b = a : enumFromThen b (b + b - a)

enumFromTo :: Int -> Int -> [Int]
enumFromTo a b = if a > b then [] else a : enumFromTo (a + 1) b

enumFromThenTo :: Int -> Int -> Int -> [Int]
enumFromThenTo a b c
  | b >= a = if a > c then [] else a : enumFromThenTo b (b + b - a) c
  | otherwise = if a < c then [] else a : enumFromThenTo b (b + b - a) c

-- ---- tuples, Maybe, Either ----

fst :: (a, b) -> a
fst (x, _) = x

snd :: (a, b) -> b
snd (_, y) = y

curry :: ((a, b) -> c) -> a -> b -> c
curry f x y = f (x, y)

uncurry :: (a -> b -> c) -> (a, b) -> c
uncurry f (x, y) = f x y

maybe :: b -> (a -> b) -> Maybe a -> b
maybe d _ Nothing = d
maybe _ f (Just x) = f x

either :: (a -> c) -> (b -> c) -> Either a b -> c
either f _ (Left x) = f x
either _ g (Right y) = g y

-- ---- lists ----

map :: (a -> b) -> [a] -> [b]
map _ [] = []
map f (x:xs) = f x : map f xs

(++) :: [a] -> [a] -> [a]
[] ++ ys = ys
(x:xs) ++ ys = x : (xs ++ ys)

filter :: (a -> Bool) -> [a] -> [a]
filter _ [] = []
filter p (x:xs) = if p x then x : filter p xs else filter p xs

head :: [a] -> a
head (x:_) = x
head [] = error "Prelude.head: empty list"

last :: [a] -> a
last [x] = x
last (_:xs) = last xs
last [] = error "Prelude.last: empty list"

tail :: [a] -> [a]
tail (_:xs) = xs
tail [] = error "Prelude.tail: empty list"

init :: [a] -> [a]
init [x] = []
init (x:xs) = x : init xs
init [] = error "Prelude.init: empty list"

null :: [a] -> Bool
null [] = True
null _ = False

length :: [a] -> Int
length xs = go 0 xs
  where go n [] = n
        go n (_:ys) = let m = n + 1 in m `seq` go m ys

(!!) :: [a] -> Int -> a
(x:xs) !! n = if n == 0 then x else xs !! (n - 1)
[] !! _ = error "Prelude.!!: index too large"

reverse :: [a] -> [a]
reverse l = rev l []
  where rev [] a = a
        rev (x:xs) a = rev xs (x : a)

foldr :: (a -> b -> b) -> b -> [a] -> b
foldr _ z [] = z
foldr f z (x:xs) = f x (foldr f z xs)

foldl :: (b -> a -> b) -> b -> [a] -> b
foldl _ z [] = z
foldl f z (x:xs) = let z' = f z x in z' `seq` foldl f z' xs

foldr1 :: (a -> a -> a) -> [a] -> a
foldr1 _ [x] = x
foldr1 f (x:xs) = f x (foldr1 f xs)
foldr1 _ [] = error "Prelude.foldr1: empty list"

foldl1 :: (a -> a -> a) -> [a] -> a
foldl1 f (x:xs) = foldl f x xs
foldl1 _ [] = error "Prelude.foldl1: empty list"

scanl :: (b -> a -> b) -> b -> [a] -> [b]
scanl f q ls = q : (case ls of
  [] -> []
  x:xs -> scanl f (f q x) xs)

and, or :: [Bool] -> Bool
and = foldr (&&) True
or = foldr (||) False

any, all :: (a -> Bool) -> [a] -> Bool
any p = or . map p
all p = and . map p

sum, product :: [Int] -> Int
sum = foldl (+) 0
product = foldl (*) 1

maximum, minimum :: [Int] -> Int
maximum = foldl1 max
minimum = foldl1 min

elem, notElem :: Int -> [Int] -> Bool
elem x = any (== x)
notElem x = all (/= x)

lookup :: Int -> [(Int, b)] -> Maybe b
lookup _ [] = Nothing
lookup k ((k', v):rest) = if k == k' then Just v else lookup k rest

concat :: [[a]] -> [a]
concat = foldr (++) []

concatMap :: (a -> [b]) -> [a] -> [b]
concatMap f = foldr ((++) . f) []

iterate :: (a -> a) -> a -> [a]
iterate f x = x : iterate f (f x)

repeat :: a -> [a]
repeat x = x : repeat x

replicate :: Int -> a -> [a]
replicate n x = take n (repeat x)

cycle :: [a] -> [a]
cycle [] = error "Prelude.cycle: empty list"
cycle xs = xs ++ cycle xs

take :: Int -> [a] -> [a]
take n _ | n <= 0 = []
take _ [] = []
take n (x:xs) = x : take (n - 1) xs

drop :: Int -> [a] -> [a]
drop n xs | n <= 0 = xs
drop _ [] = []
drop n (_:xs) = drop (n - 1) xs

splitAt :: Int -> [a] -> ([a], [a])
splitAt n xs = (take n xs, drop n xs)

takeWhile :: (a -> Bool) -> [a] -> [a]
takeWhile _ [] = []
takeWhile p (x:xs) = if p x then x : takeWhile p xs else []

dropWhile :: (a -> Bool) -> [a] -> [a]
dropWhile _ [] = []
dropWhile p xs@(x:xs') = if p x then dropWhile p xs' else xs

span, break :: (a -> Bool) -> [a] -> ([a], [a])
span _ [] = ([], [])
span p xs@(x:xs')
  | p x = let (ys, zs) = span p xs' in (x : ys, zs)
  | otherwise = ([], xs)
break p = span (not . p)

zip :: [a] -> [b] -> [(a, b)]
zip (a:as) (b:bs) = (a, b) : zip as bs
zip _ _ = []

zip3 :: [a] -> [b] -> [c] -> [(a, b, c)]
zip3 (a:as) (b:bs) (c:cs) = (a, b, c) : zip3 as bs cs
zip3 _ _ _ = []

zipWith :: (a -> b -> c) -> [a] -> [b] -> [c]
zipWith f (a:as) (b:bs) = f a b : zipWith f as bs
zipWith _ _ _ = []

unzip :: [(a, b)] -> ([a], [b])
unzip = foldr (\(a, b) ~(as, bs) -> (a : as, b : bs)) ([], [])

-- ---- strings ----

lines :: String -> [String]
lines "" = []
lines s = let (l, s') = break isNewline s
          in l : (case s' of
                    [] -> []
                    (_:s'') -> lines s'')
  where isNewline c = primCharOrd c == 10

unlines :: [String] -> String
unlines = concatMap (++ "\n")

words :: String -> [String]
words s = case dropWhile isSpace' s of
  "" -> []
  s' -> let (w, s'') = break isSpace' s' in w : words s''
  where isSpace' c = let n = primCharOrd c in n == 32 || (n >= 9 && n <= 13)

unwords :: [String] -> String
unwords [] = ""
unwords [w] = w
unwords (w:ws) = w ++ ' ' : unwords ws

-- ---- IO ----

(>>=) :: IO a -> (a -> IO b) -> IO b
(>>=) = primBindIO

(>>) :: IO a -> IO b -> IO b
m >> k = m >>= \_ -> k

return :: a -> IO a
return = primReturnIO

(=<<) :: (a -> IO b) -> IO a -> IO b
f =<< m = m >>= f

mapM_ :: (a -> IO b) -> [a] -> IO ()
mapM_ f = foldr (\x k -> f x >> k) (return ())

sequence_ :: [IO a] -> IO ()
sequence_ = foldr (>>) (return ())

putChar :: Char -> IO ()
putChar = primPutChar

putStr :: String -> IO ()
putStr = primPutStr

putStrLn :: String -> IO ()
putStrLn s = putStr s >> putChar '\n'

print :: Int -> IO ()
print n = putStrLn (show n)
