-- The functions of extern blocks take C types, pointers and `Nullable`
-- (`Maybe`) of a pointer (E0618, FEAT-0130).
module Main where

import Cirka.Extern
import Foreign.C.Types (CInt)

extern "C" where
  abs :: Maybe CInt -> CInt
  puts :: String -> IO CInt

main :: IO ()
main = return ()
