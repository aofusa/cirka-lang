-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T10598_bug.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE GeneralizedNewtypeDeriving #-}
module Main where

newtype MyMaybe a = MyMaybe (Maybe a)
  deriving (Functor, Show)

main :: IO ()
main = print $ fmap (+1) $ MyMaybe $ Just (10 :: Int)
