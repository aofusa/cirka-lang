module Main where

foreign import ccall "&free" pFree :: Int
foreign import ccall "dynamic" callIt :: Int -> Int
foreign import ccall "f" takesList :: [Int] -> Int
foreign import ccall "g" returnsMaybe :: Int -> Maybe Int
foreign import ccall "h1" h1 :: Int -> Int
foreign import ccall "h1" h2 :: Double -> Int
foreign import stdcall "k" k :: Int -> Int

main = putStr "x"
