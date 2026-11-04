{-# LANGUAGE NoImplicitPrelude #-}
-- | Primitive types and operations of aCHC (docs/design/09-libraries.md).
-- Each `foreign import prim "p"` is implemented by the Core definition
-- `prim$p` of the desugarer (crates/achc-desugar/src/prims.core).
module Cirka.Prim where

data Int
data Char
data Double
data IO a
data Bool = False | True

foreign import prim "int_add" primIntAdd :: Int -> Int -> Int
foreign import prim "int_sub" primIntSub :: Int -> Int -> Int
foreign import prim "int_mul" primIntMul :: Int -> Int -> Int
foreign import prim "int_quot" primIntQuot :: Int -> Int -> Int
foreign import prim "int_rem" primIntRem :: Int -> Int -> Int
foreign import prim "int_neg" primIntNeg :: Int -> Int
foreign import prim "int_eq" primIntEq :: Int -> Int -> Bool
foreign import prim "int_ne" primIntNe :: Int -> Int -> Bool
foreign import prim "int_lt" primIntLt :: Int -> Int -> Bool
foreign import prim "int_le" primIntLe :: Int -> Int -> Bool
foreign import prim "int_gt" primIntGt :: Int -> Int -> Bool
foreign import prim "int_ge" primIntGe :: Int -> Int -> Bool
foreign import prim "show_int" primShowInt :: Int -> [Char]

foreign import prim "dbl_add" primDoubleAdd :: Double -> Double -> Double
foreign import prim "dbl_sub" primDoubleSub :: Double -> Double -> Double
foreign import prim "dbl_mul" primDoubleMul :: Double -> Double -> Double
foreign import prim "dbl_div" primDoubleDiv :: Double -> Double -> Double
foreign import prim "dbl_neg" primDoubleNeg :: Double -> Double
foreign import prim "dbl_sqrt" primDoubleSqrt :: Double -> Double
foreign import prim "dbl_eq" primDoubleEq :: Double -> Double -> Bool
foreign import prim "dbl_lt" primDoubleLt :: Double -> Double -> Bool
foreign import prim "dbl_le" primDoubleLe :: Double -> Double -> Bool
foreign import prim "int_to_double" primIntToDouble :: Int -> Double
foreign import prim "double_to_int" primDoubleToInt :: Double -> Int
foreign import prim "show_double" primShowDouble :: Double -> [Char]

foreign import prim "char_ord" primCharOrd :: Char -> Int
foreign import prim "char_chr" primCharChr :: Int -> Char

foreign import prim "io_return" primReturnIO :: a -> IO a
foreign import prim "io_bind" primBindIO :: IO a -> (a -> IO b) -> IO b
foreign import prim "put_char" primPutChar :: Char -> IO ()
foreign import prim "put_str" primPutStr :: [Char] -> IO ()
foreign import prim "get_char" primGetByte :: IO Int
foreign import prim "exit" primExit :: Int -> IO a
foreign import prim "arg_count" primArgCount :: IO Int
foreign import prim "get_arg" primGetArg :: Int -> IO [Char]

foreign import prim "error" primError :: [Char] -> a
foreign import prim "seq" primSeq :: a -> b -> b
