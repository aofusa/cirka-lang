{-# LANGUAGE RankNTypes #-}
-- The region types of lib/cirka-base's Cirka.Region, for the type checker
-- tests (FEAT-0127).
module Cirka.Region where

newtype Arena r = Arena Int

newtype RIO r a = RIO a

newtype In r a = In Int

withRegion :: (forall r. Arena r -> RIO r a) -> a
withRegion f = case f (Arena 0) of
  RIO x -> x

allocIn :: Arena r -> a -> RIO r (In r a)
allocIn _ _ = RIO (In 0)

pureR :: a -> RIO r a
pureR = RIO
