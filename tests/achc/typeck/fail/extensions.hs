{-# LANGUAGE MultiParamTypeClasses, GeneralizedNewtypeDeriving, RankNTypes #-}
module Main where

class Convert a b where
  convert :: a -> b

class Size a where
  size :: a -> Int

instance Convert Int Bool where
  convert _ = True

instance Convert Int where
  convert _ = True

newtype Flag = Flag Bool deriving (Size)

newtype Pair = Pair Int deriving (Convert)

c :: Bool
c = convert 'c'

useId :: (forall a. a -> a) -> Int
useId f = f 1

notId :: Bool -> Bool
notId b = b

bad :: Int
bad = useId notId

main = return ()
