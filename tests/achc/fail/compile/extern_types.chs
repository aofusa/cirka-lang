-- The types of the functions of extern blocks, also through type synonyms
-- (E0619, FEAT-0130): a non-linear `OwnedPtr` parameter, an `OwnedPtr`
-- result outside `IO`, and a borrowed pointer whose scope is not the one
-- of the result (it could leave the borrow).
module Main where

import Cirka.Extern
import Foreign.C.Types (CInt)

type File = OwnedPtr CFile

type FilePtr r = BorrowedPtr r CFile

extern "C" where
  type CFile
  fclose :: OwnedPtr CFile -> IO CInt
  pclose :: File -> IO CInt
  fdup :: CInt -> OwnedPtr CFile
  fdup2 :: CInt -> Nullable File
  fgetc :: FilePtr r -> IO CInt
  fbuf :: FilePtr r -> FilePtr q

main :: IO ()
main = return ()
