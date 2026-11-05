{-# LANGUAGE KindSignatures, ScopedTypeVariables #-}
-- FEAT-0118: kind signatures on declaration heads, binders and types.
module Main where

import Data.Kind (Type)

class Container (f :: Type -> Type) where
  cmap :: (a -> b) -> f a -> f b

newtype Id (a :: *) = Id a deriving Show

instance Container Id where
  cmap f (Id a) = Id (f a)

-- A phantom parameter of a higher kind: only the signature gives its kind.
data Proxy (t :: Type -> Type) = Proxy

data App (f :: Type -> Type) a = App (f a)

proxyName :: Proxy Maybe -> String
proxyName Proxy = "Proxy Maybe"

ident :: forall (a :: Type). a -> a
ident x = (x :: a)

unApp :: App Maybe Int -> Int
unApp (App m) = maybe 0 (+ 1) (m :: (Maybe :: Type -> Type) Int)

main :: IO ()
main = do
  print (cmap (+ 1) (Id (1 :: Int)))
  putStrLn (proxyName Proxy)
  print (ident True)
  print (unApp (App (Just 41)))
