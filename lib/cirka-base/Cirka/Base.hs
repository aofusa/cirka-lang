{-# LANGUAGE NoImplicitPrelude #-}
-- | The entities of the aCHC Prelude (docs/design/09-libraries.md §1): the
-- Haskell 2010 Prelude with GHC's class hierarchy (Functor / Applicative /
-- Monad / MonadFail, Semigroup / Monoid, Foldable / Traversable).
-- `Prelude` re-exports GHC's selection of them; `Data.*` modules others.
--
-- Until FEAT-0114, `Integer` is a synonym of the 64-bit `Int` and `Float`
-- one of `Double`. Internal helpers are named `cirka...`.
module Cirka.Base
  ( module Cirka.Base
  , Int, Char, Double, IO, Bool(..)
  ) where

import Cirka.Prim

infixr 9 .
infixl 9 !!
infixr 8 ^, ^^, **
infixl 7 *, /, `quot`, `rem`, `div`, `mod`, :%
infixl 6 +, -
infixr 6 <>
infixr 5 ++
infix 4 ==, /=, <, <=, >, >=, `elem`, `notElem`
infixl 4 <$>, <$, <*>, *>, <*
infixl 3 <|>
infixr 3 &&
infixr 2 ||
infixl 1 >>=, >>
infixr 1 =<<
infixr 0 $, $!, `seq`

type String = [Char]
type Integer = Int
type Float = Double
type Rational = Ratio Integer
type ShowS = String -> String
type ReadS a = String -> [(a, String)]
type FilePath = String

data Maybe a = Nothing | Just a
  deriving (Eq, Ord, Show, Read)

data Either a b = Left a | Right b
  deriving (Eq, Ord, Show, Read)

data Ordering = LT | EQ | GT
  deriving (Eq, Ord, Show, Read, Enum, Bounded)

data Ratio a = !a :% !a

-- ---- basic functions ----

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

asTypeOf :: a -> a -> a
asTypeOf = const

error :: [Char] -> a
error = primError

-- | `error` called at a source location (the compiler rewrites calls of
-- `error` to this): the message gets GHC's call stack line.
cirkaErrorCalledAt :: [Char] -> [Char] -> a
cirkaErrorCalledAt site msg =
  primError (msg ++ "\nCallStack (from HasCallStack):\n  " ++ site)

cirkaUndefinedAt :: [Char] -> a
cirkaUndefinedAt site =
  primError ("Prelude.undefined\nCallStack (from HasCallStack):\n  " ++ site)

errorWithoutStackTrace :: [Char] -> a
errorWithoutStackTrace = primError

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

bool :: a -> a -> Bool -> a
bool f _ False = f
bool _ t True = t

-- ---- Eq and Ord ----

class Eq a where
  (==), (/=) :: a -> a -> Bool
  x /= y = not (x == y)
  x == y = not (x /= y)

class Eq a => Ord a where
  compare :: a -> a -> Ordering
  (<), (<=), (>), (>=) :: a -> a -> Bool
  max, min :: a -> a -> a
  compare x y = if x == y then EQ else if x <= y then LT else GT
  x < y = case compare x y of { LT -> True; _ -> False }
  x <= y = case compare x y of { GT -> False; _ -> True }
  x > y = case compare x y of { GT -> True; _ -> False }
  x >= y = case compare x y of { LT -> False; _ -> True }
  max x y = if x <= y then y else x
  min x y = if x <= y then x else y

comparing :: Ord b => (a -> b) -> a -> a -> Ordering
comparing f x y = compare (f x) (f y)

instance Eq Bool where
  True == True = True
  False == False = True
  _ == _ = False

instance Ord Bool where
  compare False True = LT
  compare True False = GT
  compare _ _ = EQ

instance Eq () where
  _ == _ = True

instance Ord () where
  compare _ _ = EQ

instance Eq Int where
  (==) = primIntEq
  (/=) = primIntNe

instance Ord Int where
  compare x y = if primIntLt x y then LT else if primIntEq x y then EQ else GT
  (<) = primIntLt
  (<=) = primIntLe
  (>) = primIntGt
  (>=) = primIntGe
  max x y = if primIntLe x y then y else x
  min x y = if primIntLe x y then x else y

instance Eq Char where
  c == d = primIntEq (primCharOrd c) (primCharOrd d)
  c /= d = primIntNe (primCharOrd c) (primCharOrd d)

instance Ord Char where
  compare c d = compare (primCharOrd c) (primCharOrd d)
  c < d = primIntLt (primCharOrd c) (primCharOrd d)
  c <= d = primIntLe (primCharOrd c) (primCharOrd d)
  c > d = primIntGt (primCharOrd c) (primCharOrd d)
  c >= d = primIntGe (primCharOrd c) (primCharOrd d)

instance Eq Double where
  (==) = primDoubleEq

instance Ord Double where
  compare x y = if primDoubleLt x y then LT else if primDoubleEq x y then EQ else GT
  (<) = primDoubleLt
  (<=) = primDoubleLe
  x > y = primDoubleLt y x
  x >= y = primDoubleLe y x

instance Eq a => Eq [a] where
  [] == [] = True
  (x:xs) == (y:ys) = x == y && xs == ys
  _ == _ = False

instance Ord a => Ord [a] where
  compare [] [] = EQ
  compare [] (_:_) = LT
  compare (_:_) [] = GT
  compare (x:xs) (y:ys) = case compare x y of
    EQ -> compare xs ys
    other -> other

instance (Eq a, Eq b) => Eq (a, b) where
  (a, b) == (c, d) = a == c && b == d

instance (Ord a, Ord b) => Ord (a, b) where
  compare (a, b) (c, d) = case compare a c of
    EQ -> compare b d
    other -> other

instance (Eq a, Eq b, Eq c) => Eq (a, b, c) where
  (a, b, c) == (d, e, f) = a == d && b == e && c == f

instance (Ord a, Ord b, Ord c) => Ord (a, b, c) where
  compare (a, b, c) (d, e, f) = case compare a d of
    EQ -> case compare b e of
      EQ -> compare c f
      other -> other
    other -> other

instance (Eq a, Eq b, Eq c, Eq d) => Eq (a, b, c, d) where
  (a, b, c, d) == (e, f, g, h) = a == e && b == f && c == g && d == h

instance (Ord a, Ord b, Ord c, Ord d) => Ord (a, b, c, d) where
  compare (a, b, c, d) (e, f, g, h) = compare (a, (b, c, d)) (e, (f, g, h))

instance (Eq a, Eq b, Eq c, Eq d, Eq e) => Eq (a, b, c, d, e) where
  (a, b, c, d, e) == (a', b', c', d', e') = (a, (b, c, d, e)) == (a', (b', c', d', e'))

instance (Ord a, Ord b, Ord c, Ord d, Ord e) => Ord (a, b, c, d, e) where
  compare (a, b, c, d, e) (a', b', c', d', e') = compare (a, (b, c, d, e)) (a', (b', c', d', e'))

instance (Eq a, Eq b, Eq c, Eq d, Eq e, Eq f) => Eq (a, b, c, d, e, f) where
  (a, b, c, d, e, f) == (a', b', c', d', e', f') = (a, (b, c, d, e, f)) == (a', (b', c', d', e', f'))

instance (Ord a, Ord b, Ord c, Ord d, Ord e, Ord f) => Ord (a, b, c, d, e, f) where
  compare (a, b, c, d, e, f) (a', b', c', d', e', f') = compare (a, (b, c, d, e, f)) (a', (b', c', d', e', f'))

instance (Eq a, Eq b, Eq c, Eq d, Eq e, Eq f, Eq g) => Eq (a, b, c, d, e, f, g) where
  (a, b, c, d, e, f, g) == (a', b', c', d', e', f', g') = (a, (b, c, d, e, f, g)) == (a', (b', c', d', e', f', g'))

instance (Ord a, Ord b, Ord c, Ord d, Ord e, Ord f, Ord g) => Ord (a, b, c, d, e, f, g) where
  compare (a, b, c, d, e, f, g) (a', b', c', d', e', f', g') = compare (a, (b, c, d, e, f, g)) (a', (b', c', d', e', f', g'))

-- ---- Enum and Bounded ----

class Enum a where
  succ, pred :: a -> a
  toEnum :: Int -> a
  fromEnum :: a -> Int
  enumFrom :: a -> [a]
  enumFromThen :: a -> a -> [a]
  enumFromTo :: a -> a -> [a]
  enumFromThenTo :: a -> a -> a -> [a]
  succ x = toEnum (fromEnum x + 1)
  pred x = toEnum (fromEnum x - 1)
  enumFrom x = map toEnum (cirkaIntFrom (fromEnum x))
  enumFromThen x y = map toEnum (cirkaIntFromThen (fromEnum x) (fromEnum y))
  enumFromTo x y = map toEnum (cirkaIntFromTo (fromEnum x) (fromEnum y))
  enumFromThenTo x y z = map toEnum (cirkaIntFromThenTo (fromEnum x) (fromEnum y) (fromEnum z))

class Bounded a where
  minBound, maxBound :: a

cirkaIntFrom :: Int -> [Int]
cirkaIntFrom n = n : cirkaIntFrom (primIntAdd n 1)

cirkaIntFromThen :: Int -> Int -> [Int]
cirkaIntFromThen a b = a : cirkaIntFromThen b (primIntSub (primIntAdd b b) a)

cirkaIntFromTo :: Int -> Int -> [Int]
cirkaIntFromTo a b = if primIntGt a b then [] else a : cirkaIntFromTo (primIntAdd a 1) b

cirkaIntFromThenTo :: Int -> Int -> Int -> [Int]
cirkaIntFromThenTo a b c
  | primIntGe b a = if primIntGt a c then [] else a : cirkaIntFromThenTo b (primIntSub (primIntAdd b b) a) c
  | otherwise = if primIntLt a c then [] else a : cirkaIntFromThenTo b (primIntSub (primIntAdd b b) a) c

instance Enum Int where
  succ x = x + 1
  pred x = x - 1
  toEnum x = x
  fromEnum x = x
  enumFrom = cirkaIntFrom
  enumFromThen = cirkaIntFromThen
  enumFromTo = cirkaIntFromTo
  enumFromThenTo = cirkaIntFromThenTo

instance Bounded Int where
  minBound = negate 9223372036854775807 - 1
  maxBound = 9223372036854775807

instance Enum Char where
  toEnum = primCharChr
  fromEnum = primCharOrd
  enumFrom c = enumFromTo c (primCharChr 1114111)

instance Bounded Char where
  minBound = primCharChr 0
  maxBound = primCharChr 1114111

instance Enum Bool where
  toEnum 0 = False
  toEnum 1 = True
  toEnum _ = error "Prelude.Enum.Bool.toEnum: bad argument"
  fromEnum False = 0
  fromEnum True = 1
  enumFrom x = enumFromTo x True
  enumFromThen x y = enumFromThenTo x y (if fromEnum y >= fromEnum x then True else False)

instance Bounded Bool where
  minBound = False
  maxBound = True

instance Enum () where
  toEnum _ = ()
  fromEnum _ = 0
  enumFrom _ = [()]
  enumFromThen _ _ = repeat ()

instance Bounded () where
  minBound = ()
  maxBound = ()

instance (Bounded a, Bounded b) => Bounded (a, b) where
  minBound = (minBound, minBound)
  maxBound = (maxBound, maxBound)

instance Enum Double where
  succ x = x + 1
  pred x = x - 1
  toEnum = primIntToDouble
  fromEnum = primDoubleToInt
  enumFrom x = x : enumFrom (x + 1)
  enumFromThen x y = x : enumFromThen y (y + y - x)
  enumFromTo x lim = takeWhile (<= lim + 1 / 2) (enumFrom x)
  enumFromThenTo x y lim
    | y >= x = takeWhile (<= lim + (y - x) / 2) (enumFromThen x y)
    | otherwise = takeWhile (>= lim + (y - x) / 2) (enumFromThen x y)

-- ---- numbers ----

class Num a where
  (+), (-), (*) :: a -> a -> a
  negate, abs, signum :: a -> a
  fromInteger :: Integer -> a
  x - y = x + negate y
  negate x = fromInteger 0 - x

class (Num a, Ord a) => Real a where
  toRational :: a -> Rational

class (Real a, Enum a) => Integral a where
  quot, rem, div, mod :: a -> a -> a
  quotRem, divMod :: a -> a -> (a, a)
  toInteger :: a -> Integer
  n `quot` d = fst (quotRem n d)
  n `rem` d = snd (quotRem n d)
  n `div` d = fst (divMod n d)
  n `mod` d = snd (divMod n d)
  divMod n d = if signum r == negate (signum d) then (q - 1, r + d) else qr
    where qr = quotRem n d
          q = fst qr
          r = snd qr

class Num a => Fractional a where
  (/) :: a -> a -> a
  recip :: a -> a
  fromRational :: Rational -> a
  recip x = 1 / x
  x / y = x * recip y

class Fractional a => Floating a where
  pi :: a
  exp, log, sqrt :: a -> a
  (**), logBase :: a -> a -> a
  sin, cos, tan, asin, acos, atan :: a -> a
  sinh, cosh, tanh, asinh, acosh, atanh :: a -> a
  x ** y = exp (log x * y)
  logBase x y = log y / log x
  sqrt x = x ** 0.5
  tan x = sin x / cos x
  tanh x = sinh x / cosh x
  asinh x = log (x + sqrt (x * x + 1))
  acosh x = log (x + sqrt (x * x - 1))
  atanh x = (log (1 + x) - log (1 - x)) / 2

class (Real a, Fractional a) => RealFrac a where
  properFraction :: Integral b => a -> (b, a)
  truncate, round, ceiling, floor :: Integral b => a -> b
  truncate x = fst (properFraction x)
  round x =
    let nr = properFraction x
        n = fst nr
        r = snd nr
        m = if r < 0 then n - 1 else n + 1
    in case signum (abs r - 0.5) of
         -1 -> n
         0 -> if even n then n else m
         1 -> m
         _ -> error "round default defn: Bad value"
  ceiling x = let nr = properFraction x in if snd nr > 0 then fst nr + 1 else fst nr
  floor x = let nr = properFraction x in if snd nr < 0 then fst nr - 1 else fst nr

class (RealFrac a, Floating a) => RealFloat a where
  floatDigits :: a -> Int
  isNaN, isInfinite, isNegativeZero, isIEEE :: a -> Bool
  atan2 :: a -> a -> a

instance Num Int where
  (+) = primIntAdd
  (-) = primIntSub
  (*) = primIntMul
  negate = primIntNeg
  abs x = if primIntLt x 0 then primIntNeg x else x
  signum x = if primIntLt x 0 then negate 1 else if primIntEq x 0 then 0 else 1
  fromInteger x = x

instance Real Int where
  toRational x = x :% 1

instance Integral Int where
  quot = primIntQuot
  rem = primIntRem
  div x y =
    let q = primIntQuot x y
        r = primIntRem x y
    in if (r < 0 && y > 0) || (r > 0 && y < 0) then q - 1 else q
  mod x y =
    let r = primIntRem x y
    in if (r < 0 && y > 0) || (r > 0 && y < 0) then r + y else r
  quotRem x y = (primIntQuot x y, primIntRem x y)
  divMod x y = (div x y, mod x y)
  toInteger x = x

instance Num Double where
  (+) = primDoubleAdd
  (-) = primDoubleSub
  (*) = primDoubleMul
  negate = primDoubleNeg
  abs x = if x < 0 then negate x else x
  signum x = if x < 0 then negate 1 else if x > 0 then 1 else x
  fromInteger = primIntToDouble

instance Real Double where
  toRational x = cirkaDoubleToRational x

instance Fractional Double where
  (/) = primDoubleDiv
  recip x = 1 / x
  fromRational (n :% d) = primDoubleDiv (primIntToDouble n) (primIntToDouble d)

instance Floating Double where
  pi = 3.141592653589793
  exp = primDoubleExp
  log = primDoubleLog
  sqrt = primDoubleSqrt
  x ** y = primDoublePow x y
  sin = primDoubleSin
  cos = primDoubleCos
  tan = primDoubleTan
  asin = primDoubleAsin
  acos = primDoubleAcos
  atan = primDoubleAtan
  sinh = primDoubleSinh
  cosh = primDoubleCosh
  tanh = primDoubleTanh

instance RealFrac Double where
  properFraction x =
    let n = primDoubleToInt x
    in (fromIntegral n, x - primIntToDouble n)
  truncate x = fromIntegral (primDoubleToInt x)

instance RealFloat Double where
  floatDigits _ = 53
  isNaN x = not (primDoubleEq x x)
  isInfinite x = x == 1 / 0 || x == negate 1 / 0
  isNegativeZero x = x == 0 && 1 / x < 0
  isIEEE _ = True
  atan2 = primDoubleAtan2

cirkaDoubleToRational :: Double -> Rational
cirkaDoubleToRational x = go x 1
  where go y d = if primIntToDouble (primDoubleToInt y) == y || d > 1000000000000000
                   then reduce (primDoubleToInt y) d
                   else go (y * 10) (d * 10)

-- Rationals (with 64-bit `Integer` until FEAT-0114).

instance Eq a => Eq (Ratio a) where
  (a :% b) == (c :% d) = a == c && b == d

instance Integral a => Ord (Ratio a) where
  compare (a :% b) (c :% d) = compare (a * d) (c * b)

instance Integral a => Num (Ratio a) where
  (a :% b) + (c :% d) = reduce (a * d + c * b) (b * d)
  (a :% b) - (c :% d) = reduce (a * d - c * b) (b * d)
  (a :% b) * (c :% d) = reduce (a * c) (b * d)
  negate (a :% b) = negate a :% b
  abs (a :% b) = abs a :% b
  signum (a :% _) = signum a :% 1
  fromInteger n = fromInteger n :% 1

instance Integral a => Fractional (Ratio a) where
  (a :% b) / (c :% d) = (a * d) % (b * c)
  recip (a :% b) = if a < 0 then negate b :% negate a else b :% a
  fromRational (n :% d) = fromInteger n % fromInteger d

instance Integral a => Real (Ratio a) where
  toRational (a :% b) = toInteger a :% toInteger b

instance (Show a) => Show (Ratio a) where
  showsPrec p (a :% b) = showParen (p > 7) (showsPrec 8 a . showString " % " . showsPrec 8 b)

(%) :: Integral a => a -> a -> Ratio a
x % y = reduce (x * signum y) (abs y)

reduce :: Integral a => a -> a -> Ratio a
reduce x y
  | y == 0 = error "Ratio has zero denominator"
  | otherwise = let d = gcd x y in (x `quot` d) :% (y `quot` d)

numerator :: Ratio a -> a
numerator (a :% _) = a

denominator :: Ratio a -> a
denominator (_ :% b) = b

subtract :: Num a => a -> a -> a
subtract x y = y - x

even, odd :: Integral a => a -> Bool
even n = n `rem` 2 == 0
odd n = not (even n)

gcd :: Integral a => a -> a -> a
gcd x y = gcd' (abs x) (abs y)
  where gcd' a b = if b == 0 then a else gcd' b (a `rem` b)

lcm :: Integral a => a -> a -> a
lcm x y
  | x == 0 = 0
  | y == 0 = 0
  | otherwise = abs ((x `quot` gcd x y) * y)

(^) :: (Num a, Integral b) => a -> b -> a
x0 ^ y0
  | y0 < 0 = errorWithoutStackTrace "Negative exponent"
  | y0 == 0 = 1
  | otherwise = f x0 y0
  where f x y | even y = f (x * x) (y `quot` 2)
              | y == 1 = x
              | otherwise = g (x * x) (y `quot` 2) x
        g x y z | even y = g (x * x) (y `quot` 2) z
                | y == 1 = x * z
                | otherwise = g (x * x) (y `quot` 2) (x * z)

(^^) :: (Fractional a, Integral b) => a -> b -> a
x ^^ n = if n >= 0 then x ^ n else recip (x ^ negate n)

fromIntegral :: (Integral a, Num b) => a -> b
fromIntegral x = fromInteger (toInteger x)

realToFrac :: (Real a, Fractional b) => a -> b
realToFrac x = fromRational (toRational x)

-- ---- Semigroup and Monoid ----

class Semigroup a where
  (<>) :: a -> a -> a

class Semigroup a => Monoid a where
  mempty :: a
  mappend :: a -> a -> a
  mconcat :: [a] -> a
  mappend = (<>)
  mconcat = cirkaFoldrList mappend mempty

instance Semigroup [a] where
  (<>) = (++)

instance Monoid [a] where
  mempty = []

instance Semigroup Ordering where
  LT <> _ = LT
  EQ <> y = y
  GT <> _ = GT

instance Monoid Ordering where
  mempty = EQ

instance Semigroup () where
  _ <> _ = ()

instance Monoid () where
  mempty = ()

instance Semigroup a => Semigroup (Maybe a) where
  Nothing <> b = b
  Just a <> Nothing = Just a
  Just a <> Just b = Just (a <> b)

instance Semigroup a => Monoid (Maybe a) where
  mempty = Nothing

instance (Semigroup a, Semigroup b) => Semigroup (a, b) where
  (a, b) <> (c, d) = (a <> c, b <> d)

instance (Monoid a, Monoid b) => Monoid (a, b) where
  mempty = (mempty, mempty)

-- ---- Functor, Applicative, Monad ----

class Functor f where
  fmap :: (a -> b) -> f a -> f b
  (<$) :: a -> f b -> f a
  x <$ m = fmap (\_ -> x) m

class Functor f => Applicative f where
  pure :: a -> f a
  (<*>) :: f (a -> b) -> f a -> f b
  liftA2 :: (a -> b -> c) -> f a -> f b -> f c
  (*>) :: f a -> f b -> f b
  (<*) :: f a -> f b -> f a
  liftA2 f x y = fmap f x <*> y
  a *> b = fmap (\_ y -> y) a <*> b
  a <* b = liftA2 const a b

class Applicative m => Monad m where
  (>>=) :: m a -> (a -> m b) -> m b
  (>>) :: m a -> m b -> m b
  return :: a -> m a
  m >> k = m >>= \_ -> k
  return = pure

class Monad m => MonadFail m where
  fail :: String -> m a

class Applicative f => Alternative f where
  empty :: f a
  (<|>) :: f a -> f a -> f a
  some :: f a -> f [a]
  many :: f a -> f [a]
  some v = liftA2 (:) v (many v)
  many v = some v <|> pure []

class (Alternative m, Monad m) => MonadPlus m where
  mzero :: m a
  mplus :: m a -> m a -> m a
  mzero = empty
  mplus = (<|>)

instance Alternative Maybe where
  empty = Nothing
  Nothing <|> r = r
  l <|> _ = l

instance MonadPlus Maybe

instance Alternative [] where
  empty = []
  (<|>) = (++)

instance MonadPlus []

(<$>) :: Functor f => (a -> b) -> f a -> f b
(<$>) = fmap

(=<<) :: Monad m => (a -> m b) -> m a -> m b
f =<< m = m >>= f

instance Functor [] where
  fmap = map

instance Applicative [] where
  pure x = [x]
  fs <*> xs = [f x | f <- fs, x <- xs]

instance Monad [] where
  xs >>= f = concatMap f xs

instance MonadFail [] where
  fail _ = []

instance Functor Maybe where
  fmap _ Nothing = Nothing
  fmap f (Just x) = Just (f x)

instance Applicative Maybe where
  pure = Just
  Just f <*> m = fmap f m
  Nothing <*> _ = Nothing

instance Monad Maybe where
  Just x >>= k = k x
  Nothing >>= _ = Nothing

instance MonadFail Maybe where
  fail _ = Nothing

instance Functor (Either e) where
  fmap _ (Left e) = Left e
  fmap f (Right x) = Right (f x)

instance Applicative (Either e) where
  pure = Right
  Left e <*> _ = Left e
  Right f <*> r = fmap f r

instance Monad (Either e) where
  Left e >>= _ = Left e
  Right x >>= k = k x

instance Functor IO where
  fmap f m = primBindIO m (\x -> primReturnIO (f x))

instance Applicative IO where
  pure = primReturnIO
  mf <*> mx = primBindIO mf (\f -> primBindIO mx (\x -> primReturnIO (f x)))
  ma *> mb = primBindIO ma (\_ -> mb)

instance Monad IO where
  (>>=) = primBindIO
  ma >> mb = primBindIO ma (\_ -> mb)

instance MonadFail IO where
  fail s = ioError (userError s)

instance Functor ((,) a) where
  fmap f (x, y) = (x, f y)

mapM_ :: (Foldable t, Monad m) => (a -> m b) -> t a -> m ()
mapM_ f = foldr (\x k -> f x >> k) (return ())

forM_ :: (Foldable t, Monad m) => t a -> (a -> m b) -> m ()
forM_ = flip mapM_

forM :: (Traversable t, Monad m) => t a -> (a -> m b) -> m (t b)
forM = flip mapM

void :: Functor f => f a -> f ()
void x = () <$ x

-- | Strict left fold (Data.List / Data.Foldable).
foldl' :: Foldable t => (b -> a -> b) -> b -> t a -> b
foldl' f z t = cirkaFoldlList f z (toList t)

find :: Foldable t => (a -> Bool) -> t a -> Maybe a
find p t = case filter p (toList t) of
  [] -> Nothing
  (x:_) -> Just x

maximumBy :: Foldable t => (a -> a -> Ordering) -> t a -> a
maximumBy cmp t = cirkaFoldl1List (\x y -> case cmp x y of { GT -> x; _ -> y }) (toList t)

minimumBy :: Foldable t => (a -> a -> Ordering) -> t a -> a
minimumBy cmp t = cirkaFoldl1List (\x y -> case cmp x y of { GT -> y; _ -> x }) (toList t)

sequence_ :: (Foldable t, Monad m) => t (m a) -> m ()
sequence_ = foldr (>>) (return ())

traverse_ :: (Foldable t, Applicative f) => (a -> f b) -> t a -> f ()
traverse_ f = foldr (\x k -> f x *> k) (pure ())

sequenceA_ :: (Foldable t, Applicative f) => t (f a) -> f ()
sequenceA_ = foldr (*>) (pure ())

-- ---- Foldable and Traversable ----

class Foldable t where
  foldMap :: Monoid m => (a -> m) -> t a -> m
  foldr :: (a -> b -> b) -> b -> t a -> b
  foldl :: (b -> a -> b) -> b -> t a -> b
  foldr1 :: (a -> a -> a) -> t a -> a
  foldl1 :: (a -> a -> a) -> t a -> a
  toList :: t a -> [a]
  null :: t a -> Bool
  length :: t a -> Int
  elem :: Eq a => a -> t a -> Bool
  maximum :: Ord a => t a -> a
  minimum :: Ord a => t a -> a
  sum :: Num a => t a -> a
  product :: Num a => t a -> a
  foldMap f = foldr (\x acc -> f x <> acc) mempty
  foldl f z t = cirkaFoldlList f z (toList t)
  foldr1 f t = cirkaFoldr1List f (toList t)
  foldl1 f t = cirkaFoldl1List f (toList t)
  toList t = foldr (:) [] t
  null t = foldr (\_ _ -> False) True t
  length t = cirkaLength (toList t)
  elem x t = cirkaElem x (toList t)
  maximum t = cirkaFoldl1List max (toList t)
  minimum t = cirkaFoldl1List min (toList t)
  sum t = cirkaFoldlList (+) 0 (toList t)
  product t = cirkaFoldlList (*) 1 (toList t)

class (Functor t, Foldable t) => Traversable t where
  traverse :: Applicative f => (a -> f b) -> t a -> f (t b)
  sequenceA :: Applicative f => t (f a) -> f (t a)
  mapM :: Monad m => (a -> m b) -> t a -> m (t b)
  sequence :: Monad m => t (m a) -> m (t a)
  sequenceA t = traverse id t
  mapM f t = traverse f t
  sequence t = sequenceA t

instance Foldable [] where
  foldr = cirkaFoldrList
  foldl = cirkaFoldlList
  foldr1 = cirkaFoldr1List
  foldl1 = cirkaFoldl1List
  toList xs = xs
  null [] = True
  null _ = False
  length = cirkaLength
  elem = cirkaElem
  sum = cirkaFoldlList (+) 0
  product = cirkaFoldlList (*) 1
  maximum = cirkaFoldl1List max
  minimum = cirkaFoldl1List min

instance Traversable [] where
  traverse f = cirkaFoldrList (\x ys -> liftA2 (:) (f x) ys) (pure [])

instance Foldable Maybe where
  foldr _ z Nothing = z
  foldr f z (Just x) = f x z

instance Traversable Maybe where
  traverse _ Nothing = pure Nothing
  traverse f (Just x) = fmap Just (f x)

instance Foldable (Either a) where
  foldr _ z (Left _) = z
  foldr f z (Right y) = f y z

instance Traversable (Either a) where
  traverse _ (Left x) = pure (Left x)
  traverse f (Right y) = fmap Right (f y)

instance Foldable ((,) a) where
  foldr f z (_, y) = f y z

cirkaFoldrList :: (a -> b -> b) -> b -> [a] -> b
cirkaFoldrList _ z [] = z
cirkaFoldrList f z (x:xs) = f x (cirkaFoldrList f z xs)

cirkaFoldlList :: (b -> a -> b) -> b -> [a] -> b
cirkaFoldlList _ z [] = z
cirkaFoldlList f z (x:xs) = let z' = f z x in z' `seq` cirkaFoldlList f z' xs

cirkaFoldr1List :: (a -> a -> a) -> [a] -> a
cirkaFoldr1List _ [x] = x
cirkaFoldr1List f (x:xs) = f x (cirkaFoldr1List f xs)
cirkaFoldr1List _ [] = errorWithoutStackTrace "Prelude.foldr1: empty list"

cirkaFoldl1List :: (a -> a -> a) -> [a] -> a
cirkaFoldl1List f (x:xs) = cirkaFoldlList f x xs
cirkaFoldl1List _ [] = errorWithoutStackTrace "Prelude.foldl1: empty list"

cirkaLength :: [a] -> Int
cirkaLength xs = go 0 xs
  where go n [] = n
        go n (_:ys) = let m = primIntAdd n 1 in m `seq` go m ys

cirkaElem :: Eq a => a -> [a] -> Bool
cirkaElem _ [] = False
cirkaElem x (y:ys) = x == y || cirkaElem x ys

and, or :: Foldable t => t Bool -> Bool
and = foldr (&&) True
or = foldr (||) False

any, all :: Foldable t => (a -> Bool) -> t a -> Bool
any p = foldr (\x r -> p x || r) False
all p = foldr (\x r -> p x && r) True

notElem :: (Foldable t, Eq a) => a -> t a -> Bool
notElem x t = not (elem x t)

concat :: Foldable t => t [a] -> [a]
concat = foldr (++) []

concatMap :: Foldable t => (a -> [b]) -> t a -> [b]
concatMap f = foldr (\x acc -> f x ++ acc) []

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
head [] = errorWithoutStackTrace "Prelude.head: empty list"

last :: [a] -> a
last [x] = x
last (_:xs) = last xs
last [] = errorWithoutStackTrace "Prelude.last: empty list"

tail :: [a] -> [a]
tail (_:xs) = xs
tail [] = errorWithoutStackTrace "Prelude.tail: empty list"

init :: [a] -> [a]
init [_] = []
init (x:xs) = x : init xs
init [] = errorWithoutStackTrace "Prelude.init: empty list"

(!!) :: [a] -> Int -> a
xs !! n | n < 0 = errorWithoutStackTrace "Prelude.!!: negative index"
(x:xs) !! n = if n == 0 then x else xs !! (n - 1)
[] !! _ = errorWithoutStackTrace "Prelude.!!: index too large"

reverse :: [a] -> [a]
reverse l = rev l []
  where rev [] a = a
        rev (x:xs) a = rev xs (x : a)

scanl :: (b -> a -> b) -> b -> [a] -> [b]
scanl f q ls = q : (case ls of
  [] -> []
  x:xs -> scanl f (f q x) xs)

scanl1 :: (a -> a -> a) -> [a] -> [a]
scanl1 f (x:xs) = scanl f x xs
scanl1 _ [] = []

scanr :: (a -> b -> b) -> b -> [a] -> [b]
scanr _ q0 [] = [q0]
scanr f q0 (x:xs) = case scanr f q0 xs of
  qs@(q:_) -> f x q : qs
  [] -> []

scanr1 :: (a -> a -> a) -> [a] -> [a]
scanr1 _ [] = []
scanr1 _ [x] = [x]
scanr1 f (x:xs) = case scanr1 f xs of
  qs@(q:_) -> f x q : qs
  [] -> []

iterate :: (a -> a) -> a -> [a]
iterate f x = x : iterate f (f x)

repeat :: a -> [a]
repeat x = x : repeat x

replicate :: Int -> a -> [a]
replicate n x = take n (repeat x)

cycle :: [a] -> [a]
cycle [] = errorWithoutStackTrace "Prelude.cycle: empty list"
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

lookup :: Eq a => a -> [(a, b)] -> Maybe b
lookup _ [] = Nothing
lookup k ((k', v):rest) = if k == k' then Just v else lookup k rest

zip :: [a] -> [b] -> [(a, b)]
zip (a:as) (b:bs) = (a, b) : zip as bs
zip _ _ = []

zip3 :: [a] -> [b] -> [c] -> [(a, b, c)]
zip3 (a:as) (b:bs) (c:cs) = (a, b, c) : zip3 as bs cs
zip3 _ _ _ = []

zipWith :: (a -> b -> c) -> [a] -> [b] -> [c]
zipWith f (a:as) (b:bs) = f a b : zipWith f as bs
zipWith _ _ _ = []

zipWith3 :: (a -> b -> c -> d) -> [a] -> [b] -> [c] -> [d]
zipWith3 f (a:as) (b:bs) (c:cs) = f a b c : zipWith3 f as bs cs
zipWith3 _ _ _ _ = []

unzip :: [(a, b)] -> ([a], [b])
unzip = cirkaFoldrList (\(a, b) ~(as, bs) -> (a : as, b : bs)) ([], [])

unzip3 :: [(a, b, c)] -> ([a], [b], [c])
unzip3 = cirkaFoldrList (\(a, b, c) ~(as, bs, cs) -> (a : as, b : bs, c : cs)) ([], [], [])

-- ---- strings ----

cirkaIsSpace :: Char -> Bool
cirkaIsSpace c = let n = primCharOrd c in n == 32 || (n >= 9 && n <= 13) || n == 160

cirkaIsDigit :: Char -> Bool
cirkaIsDigit c = c >= '0' && c <= '9'

cirkaIsAlpha :: Char -> Bool
cirkaIsAlpha c = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || primCharOrd c > 127

cirkaIsUpper :: Char -> Bool
cirkaIsUpper c = c >= 'A' && c <= 'Z'

lines :: String -> [String]
lines "" = []
lines s = let (l, s') = break (== '\n') s
          in l : (case s' of
                    [] -> []
                    (_:s'') -> lines s'')

unlines :: [String] -> String
unlines = concatMap (++ "\n")

words :: String -> [String]
words s = case dropWhile cirkaIsSpace s of
  "" -> []
  s' -> let (w, s'') = break cirkaIsSpace s' in w : words s''

unwords :: [String] -> String
unwords [] = ""
unwords [w] = w
unwords (w:ws) = w ++ ' ' : unwords ws

-- ---- Show ----

class Show a where
  showsPrec :: Int -> a -> ShowS
  show :: a -> String
  showList :: [a] -> ShowS
  showsPrec _ x s = show x ++ s
  show x = showsPrec 0 x ""
  showList ls s = showList__ shows ls s

showList__ :: (a -> ShowS) -> [a] -> ShowS
showList__ _ [] s = "[]" ++ s
showList__ showx (x:xs) s = '[' : showx x (showl xs)
  where showl [] = ']' : s
        showl (y:ys) = ',' : showx y (showl ys)

shows :: Show a => a -> ShowS
shows = showsPrec 0

showChar :: Char -> ShowS
showChar = (:)

showString :: String -> ShowS
showString = (++)

showParen :: Bool -> ShowS -> ShowS
showParen b p = if b then showChar '(' . p . showChar ')' else p

print :: Show a => a -> IO ()
print x = putStrLn (show x)

instance Show Int where
  showsPrec p n r = if n < 0 && p > 6 then '(' : primShowInt n ++ (')' : r) else primShowInt n ++ r
  show = primShowInt

instance Show Double where
  showsPrec p x = showParen (p > 6 && (x < 0 || isNegativeZero x)) (showString (primShowDoubleHs x))

instance Show Bool where
  showsPrec _ True = showString "True"
  showsPrec _ False = showString "False"

instance Show () where
  showsPrec _ () = showString "()"

instance Show Char where
  showsPrec _ '\'' = showString "'\\''"
  showsPrec _ c = showChar '\'' . showLitChar c . showChar '\''
  showList cs = showChar '"' . showLitString cs . showChar '"'

instance Show a => Show [a] where
  showsPrec _ = showList

instance (Show a, Show b) => Show (a, b) where
  showsPrec _ (a, b) s = cirkaShowTuple [shows a, shows b] s

instance (Show a, Show b, Show c) => Show (a, b, c) where
  showsPrec _ (a, b, c) s = cirkaShowTuple [shows a, shows b, shows c] s

instance (Show a, Show b, Show c, Show d) => Show (a, b, c, d) where
  showsPrec _ (a, b, c, d) s = cirkaShowTuple [shows a, shows b, shows c, shows d] s

instance (Show a, Show b, Show c, Show d, Show e) => Show (a, b, c, d, e) where
  showsPrec _ (a, b, c, d, e) s = cirkaShowTuple [shows a, shows b, shows c, shows d, shows e] s

instance (Show a, Show b, Show c, Show d, Show e, Show f) => Show (a, b, c, d, e, f) where
  showsPrec _ (a, b, c, d, e, f) s = cirkaShowTuple [shows a, shows b, shows c, shows d, shows e, shows f] s

instance (Show a, Show b, Show c, Show d, Show e, Show f, Show g) => Show (a, b, c, d, e, f, g) where
  showsPrec _ (a, b, c, d, e, f, g) s =
    cirkaShowTuple [shows a, shows b, shows c, shows d, shows e, shows f, shows g] s

cirkaShowTuple :: [ShowS] -> ShowS
cirkaShowTuple ss = showChar '(' . cirkaFoldr1List (\s r -> s . showChar ',' . r) ss . showChar ')'

showLitChar :: Char -> ShowS
showLitChar c s
  | primCharOrd c > 127 = showChar '\\' (cirkaProtectEsc cirkaIsDigit (shows (primCharOrd c)) s)
showLitChar '\DEL' s = showString "\\DEL" s
showLitChar '\\' s = showString "\\\\" s
showLitChar c s | c >= ' ' = showChar c s
showLitChar '\a' s = showString "\\a" s
showLitChar '\b' s = showString "\\b" s
showLitChar '\f' s = showString "\\f" s
showLitChar '\n' s = showString "\\n" s
showLitChar '\r' s = showString "\\r" s
showLitChar '\t' s = showString "\\t" s
showLitChar '\v' s = showString "\\v" s
showLitChar '\SO' s = cirkaProtectEsc (== 'H') (showString "\\SO") s
showLitChar c s = showString ('\\' : (cirkaAsciiTab !! primCharOrd c)) s

showLitString :: String -> ShowS
showLitString [] s = s
showLitString ('"' : cs) s = showString "\\\"" (showLitString cs s)
showLitString (c : cs) s = showLitChar c (showLitString cs s)

cirkaProtectEsc :: (Char -> Bool) -> ShowS -> ShowS
cirkaProtectEsc p f = f . cont
  where cont s@(c:_) | p c = "\\&" ++ s
        cont s = s

cirkaAsciiTab :: [String]
cirkaAsciiTab =
  [ "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL"
  , "BS", "HT", "LF", "VT", "FF", "CR", "SO", "SI"
  , "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB"
  , "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US"
  , "SP" ]

-- ---- Read ----

class Read a where
  readsPrec :: Int -> ReadS a
  readList :: ReadS [a]
  readList = cirkaReadList

cirkaReadList :: Read a => ReadS [a]
cirkaReadList = readParen False (\r -> [pr | ("[", s) <- lex r, pr <- readl s])
  where readl s = [([], t) | ("]", t) <- lex s] ++
                  [(x : xs, u) | (x, t) <- reads s, (xs, u) <- readl' t]
        readl' s = [([], t) | ("]", t) <- lex s] ++
                   [(x : xs, v) | (",", t) <- lex s, (x, u) <- reads t, (xs, v) <- readl' u]

reads :: Read a => ReadS a
reads = readsPrec 0

read :: Read a => String -> a
read s = case [x | (x, t) <- reads s, ("", "") <- lex t] of
  [x] -> x
  [] -> errorWithoutStackTrace "Prelude.read: no parse"
  _ -> errorWithoutStackTrace "Prelude.read: ambiguous parse"

readParen :: Bool -> ReadS a -> ReadS a
readParen b g = if b then mandatory else optional
  where optional r = g r ++ mandatory r
        mandatory r = [(x, u) | ("(", s) <- lex r, (x, t) <- optional s, (")", u) <- lex t]

lex :: ReadS String
lex "" = [("", "")]
lex (c:s) | cirkaIsSpace c = lex (dropWhile cirkaIsSpace s)
lex ('\'':s) = [('\'' : ch ++ "'", t) | (ch, '\'' : t) <- cirkaLexLitChar s, ch /= "'"]
lex ('"':s) = [('"' : str, t) | (str, t) <- lexString s]
  where lexString ('"':s') = [("\"", s')]
        lexString s' = [(ch ++ str, u) | (ch, t) <- lexStrItem s', (str, u) <- lexString t]
        lexStrItem ('\\':'&':s') = [("\\&", s')]
        lexStrItem ('\\':c':s') | cirkaIsSpace c' = [("\\&", t) | '\\' : t <- [dropWhile cirkaIsSpace s']]
        lexStrItem s' = cirkaLexLitChar s'
lex (c:s)
  | isSingle c = [([c], s)]
  | isSym c = [(c : sym, t) | (sym, t) <- [span isSym s]]
  | cirkaIsAlpha c || c == '_' = [(c : nam, t) | (nam, t) <- [span isIdChar s]]
  | cirkaIsDigit c = [(c : ds ++ fe, t) | (ds, s') <- [span cirkaIsDigit s], (fe, t) <- lexFracExp s']
  | otherwise = []
  where isSingle x = x `elem` ",;()[]{}`"
        isSym x = x `elem` "!@#$%&*+./<=>?\\^|:-~"
        isIdChar x = cirkaIsAlpha x || cirkaIsDigit x || x == '_' || x == '\''
        lexFracExp ('.':d:ds) | cirkaIsDigit d = [('.' : d : ds' ++ e, u) | (ds', t) <- [span cirkaIsDigit ds], (e, u) <- lexExp t]
        lexFracExp s' = lexExp s'
        lexExp (e:s') | e == 'e' || e == 'E' =
          [(e : c' : ds, u) | (c' : t) <- [s'], c' == '+' || c' == '-', (ds, u) <- lexDigits t] ++
          [(e : ds, t) | (ds, t) <- lexDigits s']
        lexExp s' = [("", s')]
        lexDigits s' = [(ds, t) | (ds, t) <- [span cirkaIsDigit s'], not (null ds)]

cirkaLexLitChar :: ReadS String
cirkaLexLitChar ('\\':s) = map (\(t, r) -> ('\\' : t, r)) (lexEsc s)
  where lexEsc (c:s')
          | c `elem` "abfnrtv\\\"'" = [([c], s')]
        lexEsc ('^':c:s') | c >= '@' && c <= '_' = [(['^', c], s')]
        lexEsc s'@(d:_) | cirkaIsDigit d = [span cirkaIsDigit s']
        lexEsc s'@(c:_) | cirkaIsUpper c =
          case [(mne, drop (length mne) s') | mne <- "DEL" : cirkaAsciiTab, mne == take (length mne) s'] of
            (pr:_) -> [pr]
            [] -> []
        lexEsc _ = []
cirkaLexLitChar (c:s) = [([c], s)]
cirkaLexLitChar "" = []

cirkaReadDec :: Num a => ReadS a
cirkaReadDec s = case span cirkaIsDigit s of
  ([], _) -> []
  (ds, r) -> [(cirkaFoldlList (\n d -> n * 10 + fromInteger (primCharOrd d - 48)) 0 ds, r)]

-- | `readSigned` of the Haskell 2010 report (the precedence is ignored).
cirkaReadSigned :: Num a => ReadS a -> Int -> ReadS a
cirkaReadSigned readPos _ = readParen False read'
  where read' r = read'' r ++ [(negate x, t) | ("-", s) <- lex r, (x, t) <- read'' s]
        read'' r = [(n, s) | (str, s) <- lex r, (n, "") <- readPos str]

instance Read Int where
  readsPrec = cirkaReadSigned cirkaReadDec

instance Read Double where
  readsPrec = cirkaReadSigned readD
    where readD s = case s of
            "NaN" -> [(0 / 0, "")]
            "Infinity" -> [(1 / 0, "")]
            (c:_) | cirkaIsDigit c -> [(primReadDouble s, "")]
            _ -> []

instance Read Char where
  readsPrec _ = readParen False (\r -> [(c, t) | ('\'' : s, t) <- lex r, (c, "\'") <- cirkaReadLitChar s])
  readList = readParen False (\r -> [(l, t) | ('"' : s, t) <- lex r, (l, _) <- readl s])
    where readl ('"':s) = [("", s)]
          readl ('\\':'&':s) = readl s
          readl s = [(c : cs, u) | (c, t) <- cirkaReadLitChar s, (cs, u) <- readl t]

cirkaReadLitChar :: ReadS Char
cirkaReadLitChar ('\\':s) = readEsc s
  where readEsc ('a':t) = [('\a', t)]
        readEsc ('b':t) = [('\b', t)]
        readEsc ('f':t) = [('\f', t)]
        readEsc ('n':t) = [('\n', t)]
        readEsc ('r':t) = [('\r', t)]
        readEsc ('t':t) = [('\t', t)]
        readEsc ('v':t) = [('\v', t)]
        readEsc ('\\':t) = [('\\', t)]
        readEsc ('"':t) = [('"', t)]
        readEsc ('\'':t) = [('\'', t)]
        readEsc t@(d:_) | cirkaIsDigit d = [(primCharChr n, u) | (n, u) <- cirkaReadDec t]
        readEsc t = [(primCharChr i, drop (length mne) t)
                    | (i, mne) <- zip (cirkaFromTo 0 32) cirkaAsciiTab ++ [(127, "DEL")]
                    , mne == take (length mne) t]
cirkaReadLitChar (c:s) = [(c, s)]
cirkaReadLitChar [] = []

cirkaFromTo :: Int -> Int -> [Int]
cirkaFromTo = cirkaIntFromTo

instance Read Bool where
  readsPrec _ r = [(False, t) | ("False", t) <- lex r] ++ [(True, t) | ("True", t) <- lex r]

instance Read () where
  readsPrec _ = readParen False (\r -> [((), t) | ("(", s) <- lex r, (")", t) <- lex s])

instance Read a => Read [a] where
  readsPrec _ = readList

instance (Read a, Read b) => Read (a, b) where
  readsPrec _ = readParen False (\r -> [((a, b), w) | ("(", s) <- lex r, (a, t) <- reads s,
                                                      (",", u) <- lex t, (b, v) <- reads u,
                                                      (")", w) <- lex v])

instance (Read a, Read b, Read c) => Read (a, b, c) where
  readsPrec _ = readParen False (\r -> [((a, b, c), y) | ("(", s) <- lex r, (a, t) <- reads s,
                                                         (",", u) <- lex t, (b, v) <- reads u,
                                                         (",", w) <- lex v, (c, x) <- reads w,
                                                         (")", y) <- lex x])

-- ---- IO ----

data IOError = IOError String

instance Show IOError where
  showsPrec _ (IOError s) = showString s

ioError :: IOError -> IO a
ioError (IOError s) = primError s

userError :: String -> IOError
userError s = IOError ("user error (" ++ s ++ ")")

putChar :: Char -> IO ()
putChar = primPutChar

putStr :: String -> IO ()
putStr = primPutStr

cirkaPutErr :: String -> IO ()
cirkaPutErr = primPutErrStr

putStrLn :: String -> IO ()
putStrLn s = primBindIO (putStr s) (\_ -> putChar '\n')

-- | Reads a UTF-8 encoded character; fails at the end of input.
getChar :: IO Char
getChar = primBindIO primGetByte (\b ->
  if b < 0 then ioError (IOError "<stdin>: hGetChar: end of file")
  else if b < 128 then return (primCharChr b)
  else let n = if b >= 240 then 3 else if b >= 224 then 2 else 1
           lead = if b >= 240 then b - 240 else if b >= 224 then b - 224 else b - 192
       in fmap primCharChr (cirkaUtf8Cont n lead))

cirkaUtf8Cont :: Int -> Int -> IO Int
cirkaUtf8Cont 0 acc = return acc
cirkaUtf8Cont n acc = primBindIO primGetByte (\b -> cirkaUtf8Cont (n - 1) (acc * 64 + (b - 128)))

-- | One line of stdin without its newline; fails at the end of input.
getLine :: IO String
getLine = primBindIO primGetByte (\b ->
  if b < 0 then ioError (IOError "<stdin>: hGetLine: end of file")
  else cirkaGetLine b)

cirkaGetLine :: Int -> IO String
cirkaGetLine b
  | b < 0 || b == 10 = return []
  | otherwise = do
      c <- cirkaDecode b
      n <- primGetByte
      cs <- cirkaGetLine n
      return (c : cs)

cirkaDecode :: Int -> IO Char
cirkaDecode b
  | b < 128 = return (primCharChr b)
  | otherwise =
      let n = if b >= 240 then 3 else if b >= 224 then 2 else 1
          lead = if b >= 240 then b - 240 else if b >= 224 then b - 224 else b - 192
      in fmap primCharChr (cirkaUtf8Cont n lead)

-- | The rest of the input (read eagerly).
getContents :: IO String
getContents = primBindIO primGetByte (\b ->
  if b < 0 then return []
  else do
    c <- cirkaDecode b
    cs <- getContents
    return (c : cs))

-- | Reads the whole file (strictly; GHC reads it lazily).
readFile :: FilePath -> IO String
readFile = primReadFile

writeFile :: FilePath -> String -> IO ()
writeFile p s = primWriteFile p s 0

appendFile :: FilePath -> String -> IO ()
appendFile p s = primWriteFile p s 1

interact :: (String -> String) -> IO ()
interact f = getContents >>= \s -> putStr (f s)

readLn :: Read a => IO a
readLn = getLine >>= readIO

readIO :: Read a => String -> IO a
readIO s = case [x | (x, t) <- reads s, ("", "") <- lex t] of
  [x] -> return x
  [] -> ioError (userError "Prelude.readIO: no parse")
  _ -> ioError (userError "Prelude.readIO: ambiguous parse")
