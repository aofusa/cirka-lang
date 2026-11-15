module Main where

-- Function pointers in foreign imports (FEAT-0047): the address of a C
-- function, a call through a pointer, and a pointer made of a Haskell
-- function (an extern `:callback` with a handler that boxes the C
-- arguments, runs the action and unboxes the result).
import Cirka.Foreign

foreign import ccall "&labs" pLabs :: FunPtr (Int -> Int)
foreign import ccall "stdlib.h &environ" environ :: Ptr (Ptr Char)
foreign import ccall "dynamic" callInt :: FunPtr (Int -> Int) -> Int -> Int
foreign import ccall "wrapper" mkStep :: (Int -> Double -> IO Double) -> IO (FunPtr (Int -> Double -> IO Double))
foreign import ccall "wrapper" mkAction :: (Ptr a -> IO ()) -> IO (FunPtr (Ptr a -> IO ()))

main = putStr "x"
