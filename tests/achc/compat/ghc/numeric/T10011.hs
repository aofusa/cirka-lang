-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T10011.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE ScopedTypeVariables, TypeOperators, GADTs #-}
module Main
       ( main -- :: IO ()
       ) where
import Data.Data
import Data.Ratio

main :: IO ()
main =
  let bad = gmapT (\(x :: b) ->
              case eqT :: Maybe (b :~: Integer) of
                Nothing -> x;
                Just Refl -> x * 2) (1 % 2) :: Rational
  in print (bad == numerator bad % denominator bad)
