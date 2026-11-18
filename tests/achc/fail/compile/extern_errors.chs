-- Errors of extern blocks that the renamer reports (E0619, FEAT-0130): a
-- borrowed pointer written with its scope, and the misuses of `free with`.
module Main where

import Cirka.Extern
import Foreign.C.Types (CInt)

extern "C" where
  type CFile
  type CDir
  fgetc :: BorrowedPtr r CFile -> IO CInt
  fopen :: IO (OwnedPtr CFile) free with pclose
  malloc :: CInt -> IO (OwnedPtr CInt) free with free
  free :: OwnedPtr CInt %1 -> IO ()
  opendir :: IO (Nullable (OwnedPtr CDir)) free with closedir
  fdopendir :: CInt -> IO (Nullable (OwnedPtr CDir)) free with closedir2
  dirfd :: IO (OwnedPtr CDir) free with ftell
  closedir :: OwnedPtr CDir %1 -> IO CInt
  closedir2 :: OwnedPtr CDir %1 -> IO CInt
  ftell :: BorrowedPtr CDir -> IO CInt

main :: IO ()
main = return ()
