-- Extern blocks support only the C calling convention (E0619, FEAT-0130).
module Main where

import Foreign.C.Types (CInt)

extern "rust" where
  abs :: CInt -> CInt

main :: IO ()
main = return ()
