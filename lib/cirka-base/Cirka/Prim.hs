{-# LANGUAGE NoImplicitPrelude #-}
-- | Primitive types and operations of aCHC (docs/design/09-libraries.md).
-- Each `foreign import prim "p"` is implemented by the Core definition
-- `prim$p` of the desugarer (crates/achc-desugar/src/prims.core).
module Cirka.Prim where

data Int
data Char
data Double
data IO a
data IORef a
data BigNat
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
foreign import prim "dbl_show_hs" primShowDoubleHs :: Double -> [Char]
foreign import prim "dbl_parse" primReadDouble :: [Char] -> Double
foreign import prim "dbl_exp" primDoubleExp :: Double -> Double
foreign import prim "dbl_log" primDoubleLog :: Double -> Double
foreign import prim "dbl_sin" primDoubleSin :: Double -> Double
foreign import prim "dbl_cos" primDoubleCos :: Double -> Double
foreign import prim "dbl_tan" primDoubleTan :: Double -> Double
foreign import prim "dbl_asin" primDoubleAsin :: Double -> Double
foreign import prim "dbl_acos" primDoubleAcos :: Double -> Double
foreign import prim "dbl_atan" primDoubleAtan :: Double -> Double
foreign import prim "dbl_sinh" primDoubleSinh :: Double -> Double
foreign import prim "dbl_cosh" primDoubleCosh :: Double -> Double
foreign import prim "dbl_tanh" primDoubleTanh :: Double -> Double
foreign import prim "dbl_atan2" primDoubleAtan2 :: Double -> Double -> Double
foreign import prim "dbl_pow" primDoublePow :: Double -> Double -> Double

foreign import prim "char_ord" primCharOrd :: Char -> Int
foreign import prim "char_chr" primCharChr :: Int -> Char

foreign import prim "io_return" primReturnIO :: a -> IO a
foreign import prim "io_bind" primBindIO :: IO a -> (a -> IO b) -> IO b
foreign import prim "put_char" primPutChar :: Char -> IO ()
foreign import prim "put_str" primPutStr :: [Char] -> IO ()
foreign import prim "put_err_str" primPutErrStr :: [Char] -> IO ()
foreign import prim "flush" primFlush :: IO ()
foreign import prim "peek_byte" primPeekByte :: IO Int
foreign import prim "prog_name" primProgName :: IO [Char]
foreign import prim "ioref_new" primNewIORef :: a -> IO (IORef a)
foreign import prim "ioref_read" primReadIORef :: IORef a -> IO a
foreign import prim "ioref_write" primWriteIORef :: IORef a -> a -> IO ()
foreign import prim "read_file" primReadFile :: [Char] -> IO [Char]
foreign import prim "write_file" primWriteFile :: [Char] -> [Char] -> Int -> IO ()
foreign import prim "get_char" primGetByte :: IO Int
foreign import prim "exit" primExit :: Int -> IO a
foreign import prim "arg_count" primArgCount :: IO Int
foreign import prim "get_arg" primGetArg :: Int -> IO [Char]

foreign import prim "error" primError :: [Char] -> a
foreign import prim "seq" primSeq :: a -> b -> b

-- Integer (FEAT-0114): big integers in the runtime (cirka-bigint).
foreign import prim "big_from_int" primBigFromInt :: Int -> BigNat
foreign import prim "big_fits_int" primBigFitsInt :: BigNat -> Bool
foreign import prim "big_to_int" primBigToInt :: BigNat -> Int
foreign import prim "big_to_double" primBigToDouble :: BigNat -> Double
foreign import prim "big_from_double" primBigFromDouble :: Double -> BigNat
-- 0 add, 1 sub, 2 mul, 3 quot, 4 rem (the divisor is not zero).
foreign import prim "big_op" primBigOp :: Int -> BigNat -> BigNat -> BigNat
foreign import prim "big_cmp" primBigCmp :: BigNat -> BigNat -> Int
foreign import prim "big_show" primBigShow :: BigNat -> [Char]
-- Decimal digits (checked by the caller), negated when the flag is 1.
foreign import prim "big_read" primBigRead :: [Char] -> Int -> BigNat

-- Float as a Double rounded to single precision; Double bits.
foreign import prim "flt_round" primFloatRound :: Double -> Double
foreign import prim "flt_show_hs" primShowFloatHs :: Double -> [Char]
foreign import prim "dbl_bits" primDoubleBits :: Double -> Int
foreign import prim "dbl_from_bits" primDoubleFromBits :: Int -> Double
