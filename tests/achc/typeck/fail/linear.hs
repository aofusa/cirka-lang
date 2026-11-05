{-# LANGUAGE LinearTypes #-}
-- Linearity errors (FEAT-0120): E0302 not consumed, E0303 used twice.
module Main where

dup :: a %1 -> (a, a)
dup x = (x, x)

drop1 :: a %1 -> ()
drop1 _ = ()

unused :: a %1 -> b %1 -> a
unused x y = x

viaUnrestricted :: Int %1 -> Int
viaUnrestricted n = negate n

branches :: Bool -> a %1 -> a %1 -> a
branches b x y = if b then x else y

inLambda :: (a %1 -> (a, a))
inLambda = \v -> (v, v)

caseField :: (a, b) %1 -> a
caseField p = case p of
  (a, _) -> a

erased :: a %0 -> a
erased x = x

letTwice :: a %1 -> (a, a)
letTwice x = let y = x in (y, y)

main :: IO ()
main = return ()
