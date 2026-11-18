-- ghc: skip
-- extern "C" blocks (FEAT-0130): a C `FILE*` is an owned pointer that
-- `free with fclose` closes at the end of the IO block that holds it
-- (RAII), and that C functions borrow inside `withBorrowedIO`. NULL comes
-- back as `Nothing`.
module Main where

import Cirka.Extern
import Cirka.Ownership (returnL)
import Data.Char (chr, ord)
import Foreign.C.String (CString, newCString)
import Foreign.C.Types (CInt)
import Foreign.Marshal.Alloc (free)

extern "C" where
  type CFile
  fopen :: CString -> CString -> IO (Nullable (OwnedPtr CFile)) free with fclose
  tmpfile :: IO (Nullable (OwnedPtr CFile)) free with fclose
  fclose :: OwnedPtr CFile %1 -> IO CInt
  fputc :: CInt -> BorrowedPtr CFile -> IO CInt
  fgetc :: BorrowedPtr CFile -> IO CInt
  rewind :: BorrowedPtr CFile -> IO ()

open :: String -> String -> IO (Maybe (OwnedPtr CFile))
open path mode = do
  p <- newCString path
  m <- newCString mode
  r <- fopen p m
  free p
  free m
  returnL r

writeAll :: String -> BorrowedPtr r CFile -> BorrowIO r ()
writeAll s h = mapM_ (\c -> fputc (fromIntegral (ord c)) h) s

readAll :: BorrowedPtr r CFile -> BorrowIO r String
readAll h = do
  c <- fgetc h
  if c < 0
    then return ""
    else do
      rest <- readAll h
      return (chr (fromIntegral c) : rest)

main :: IO ()
main = do
  missing <- open "/nonexistent-dir/file.txt" "r"
  case missing of
    Nothing -> putStrLn "fopen: Nothing"
    Just f -> do
      putStrLn "fopen: opened"
  t <- tmpfile
  case t of
    Nothing -> putStrLn "tmpfile: Nothing"
    Just f -> do
      (f, s) <- withBorrowedIO f (\h -> writeAll "hello, extern\n" h >> rewind h >> readAll h)
      putStr s
  u <- tmpfile
  case u of
    Nothing -> putStrLn "tmpfile: Nothing"
    Just g -> do
      (g, ()) <- withBorrowedIO g (writeAll "x")
      n <- fclose g
      putStrLn ("fclose: " ++ show n)
