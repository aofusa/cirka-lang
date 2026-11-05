{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE KindSignatures #-}
-- The classes of lib/cirka-base's Cirka.Ownership, for the type checker
-- tests (FEAT-0122).
module Cirka.Ownership (Drop (..), Release (..)) where

class Drop (a :: LType) where
  dropIt :: a %1 -> ()

class Release (a :: LType) where
  release :: a %1 -> IO ()
