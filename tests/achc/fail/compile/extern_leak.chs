-- An owned C pointer with no `Release` instance (no `free with`) must be
-- given to the C function that frees it: forgetting it is E0302, also when
-- a `case` alternative binds it (FEAT-0130).
module Main where

import Cirka.Extern
import Foreign.C.Types (CInt)

extern "C" where
  type CFile
  tmpfile :: IO (Nullable (OwnedPtr CFile))
  fclose :: OwnedPtr CFile %1 -> IO CInt

main :: IO ()
main = do
  t <- tmpfile
  case t of
    Nothing -> return ()
    Just f -> do
      putStrLn "opened"
