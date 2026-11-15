module Main where

import Cirka.Foreign

foreign import ccall "&free" pFree :: Int
foreign import ccall "dynamic" callIt :: Int -> Int
foreign import ccall "wrapper" wrapPure :: (Int -> Int) -> FunPtr (Int -> Int)
foreign import ccall "wrapper" wrapOther :: (Int -> Int) -> IO (FunPtr (Int -> Double))
foreign import ccall "wrapper" wrapList :: ([Int] -> IO ()) -> IO (FunPtr ([Int] -> IO ()))
foreign import ccall "f" takesList :: [Int] -> Int
foreign import ccall "g" returnsMaybe :: Int -> Maybe Int
foreign import ccall "h1" h1 :: Int -> Int
foreign import ccall "h1" h2 :: Double -> Int
foreign import stdcall "k" k :: Int -> Int

main = putStr "x"
