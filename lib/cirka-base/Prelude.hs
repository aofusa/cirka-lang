{-# LANGUAGE NoImplicitPrelude #-}
-- | The Prelude of aCHC: the names GHC's Prelude exports, defined in
-- `Cirka.Base` (docs/design/09-libraries.md §1).
module Prelude
  ( -- * Basic data types
    Bool(..), (&&), (||), not, otherwise
  , Maybe(..), maybe
  , Either(..), either
  , Ordering(..)
  , Char, String
  , fst, snd, curry, uncurry
    -- * Basic classes
  , Eq((==), (/=))
  , Ord(compare, (<), (<=), (>=), (>), max, min)
  , Enum(succ, pred, toEnum, fromEnum, enumFrom, enumFromThen, enumFromTo, enumFromThenTo)
  , Bounded(minBound, maxBound)
    -- * Numbers
  , Int, Integer, Float, Double, Rational
  , Num((+), (-), (*), negate, abs, signum, fromInteger)
  , Real(toRational)
  , Integral(quot, rem, div, mod, quotRem, divMod, toInteger)
  , Fractional((/), recip, fromRational)
  , Floating(pi, exp, log, sqrt, (**), logBase, sin, cos, tan, asin, acos, atan,
             sinh, cosh, tanh, asinh, acosh, atanh)
  , RealFrac(properFraction, truncate, round, ceiling, floor)
  , RealFloat(floatDigits, isNaN, isInfinite, isNegativeZero, isIEEE, atan2)
  , subtract, even, odd, gcd, lcm, (^), (^^), fromIntegral, realToFrac
    -- * Semigroups, monoids, functors and monads
  , Semigroup((<>))
  , Monoid(mempty, mappend, mconcat)
  , Functor(fmap, (<$)), (<$>)
  , Applicative(pure, (<*>), (*>), (<*), liftA2)
  , Monad((>>=), (>>), return)
  , MonadFail(fail)
  , mapM_, sequence_, (=<<)
    -- * Folds and traversals
  , Foldable(elem, foldMap, foldr, foldl, foldr1, foldl1, maximum, minimum, product, sum,
             null, length)
  , Traversable(traverse, sequenceA, mapM, sequence)
    -- * Miscellaneous functions
  , id, const, (.), flip, ($), until, asTypeOf, error, errorWithoutStackTrace, undefined
  , seq, ($!)
    -- * Lists
  , map, (++), filter, head, last, tail, init, (!!), reverse
  , and, or, any, all, concat, concatMap
  , scanl, scanl1, scanr, scanr1, iterate, repeat, replicate, cycle
  , take, drop, takeWhile, dropWhile, span, break, splitAt
  , notElem, lookup
  , zip, zip3, zipWith, zipWith3, unzip, unzip3
  , lines, words, unlines, unwords
    -- * Converting to and from strings
  , ShowS, Show(showsPrec, showList, show), shows, showChar, showString, showParen
  , ReadS, Read(readsPrec, readList), reads, readParen, read, lex
    -- * Input and output
  , IO, putChar, putStr, putStrLn, print, getChar, getLine, getContents, interact
  , FilePath, readIO, readLn
  , IOError, ioError, userError
  ) where

import Cirka.Prim
import Cirka.Base
