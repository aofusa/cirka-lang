{-# LANGUAGE NoImplicitPrelude #-}
-- A minimal primitive module for the type checker tests.
module Cirka.Prim where

data Int
data Char
data Double
data IO a
data Bool = False | True

foreign import prim "int_add" primIntAdd :: Int -> Int -> Int
foreign import prim "int_sub" primIntSub :: Int -> Int -> Int
foreign import prim "int_eq" primIntEq :: Int -> Int -> Bool
foreign import prim "io_return" primReturnIO :: a -> IO a
foreign import prim "io_bind" primBindIO :: IO a -> (a -> IO b) -> IO b
foreign import prim "put_str" putStr :: [Char] -> IO ()
