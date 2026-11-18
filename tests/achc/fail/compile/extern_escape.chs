-- A borrowed pointer, or a computation using it, cannot leave the
-- `withBorrowedIO` / `withBorrowed` that lends it (E0320, FEAT-0130).
module Main where

import Cirka.Extern
import Cirka.Ownership (returnL)
import Foreign.C.Types (CInt, CLong)

extern "C" where
  type CFile
  tmpfile :: IO (OwnedPtr CFile) free with fclose
  fclose :: OwnedPtr CFile %1 -> IO CInt
  fgetc :: BorrowedPtr CFile -> IO CInt
  ftell :: BorrowedPtr CFile -> CLong

leakPtr :: IO ()
leakPtr = do
  f <- tmpfile
  (f, h) <- withBorrowedIO f (\h -> return h)
  return ()

leakAction :: IO ()
leakAction = do
  f <- tmpfile
  (f, a) <- withBorrowedIO f (\h -> return (fgetc h))
  return ()

leakPure :: IO ()
leakPure = do
  f <- tmpfile
  (f, b) <- returnL (withBorrowed f (\h -> return (ftell h)))
  return ()

main :: IO ()
main = leakPtr >> leakAction >> leakPure
