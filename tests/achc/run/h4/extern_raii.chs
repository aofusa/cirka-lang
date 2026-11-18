-- ghc: skip
-- extern "C" blocks (FEAT-0130): `free with` makes an owned pointer an
-- RAII value, released at the end of the IO block that holds it (also of a
-- `case` alternative that binds it) in reverse binding order; a C function
-- that takes it over consumes it; `withBorrowed` lends it to pure C
-- functions, whose borrowed results keep its scope; NULL is `Nothing`, as
-- a result and as an argument. The C side (extern_raii.c) counts the live
-- resources and records the order in which they are closed.
module Main where

import Cirka.Extern
import Cirka.Ownership (returnL)
import Foreign.C.Types (CLong)

extern "C" where
  type Res
  res_open :: CLong -> IO (Nullable (OwnedPtr Res)) free with res_close
  res_close :: OwnedPtr Res %1 -> IO CLong
  res_close_maybe :: Nullable (OwnedPtr Res) %1 -> IO CLong
  res_self :: BorrowedPtr Res -> BorrowedPtr Res
  res_if :: CLong -> BorrowedPtr Res -> Nullable (BorrowedPtr Res)
  res_id :: BorrowedPtr Res -> CLong
  res_live :: IO CLong
  res_closed :: CLong -> IO CLong
  res_reset :: IO ()

live :: String -> IO ()
live at = do
  n <- res_live
  putStrLn (at ++ ": " ++ show n ++ " live")

-- A resource that must exist.
openRes :: CLong -> IO (OwnedPtr Res)
openRes n = do
  m <- res_open n
  case m of
    Nothing -> error "res_open failed"
    Just r -> returnL r

block :: IO ()
block = do
  a <- openRes 1
  b <- openRes 2
  live "in the block"
  (a, i) <- returnL (withBorrowed a res_id)
  putStrLn ("borrowed id: " ++ show i)
  c <- openRes 3
  n <- res_close c
  putStrLn ("closed explicitly: " ++ show n)

-- Nullable arguments and borrowed results.
nullable :: IO ()
nullable = do
  n <- res_close_maybe Nothing
  putStrLn ("res_close_maybe Nothing: " ++ show n)
  m <- res_open 5
  k <- res_close_maybe m
  putStrLn ("res_close_maybe (res_open 5): " ++ show k)
  r <- openRes 6
  (r, ids) <- returnL (withBorrowed r (\h -> do
    i <- res_self h >>= res_id
    yes <- res_if 6 h
    no <- res_if 7 h
    j <- maybe (return (-1)) res_id yes
    k <- maybe (return (-1)) res_id no
    return [i, j, k]))
  putStrLn ("borrowed results: " ++ unwords (map show ids))

closedOrder :: CLong -> IO [CLong]
closedOrder k = do
  i <- res_closed k
  if i < 0
    then return []
    else do
      rest <- closedOrder (k + 1)
      return (i : rest)

main :: IO ()
main = do
  res_reset
  m <- res_open (-1)
  case m of
    Nothing -> putStrLn "res_open (-1): Nothing"
    Just r -> do
      putStrLn "res_open (-1): opened"
  block
  live "after the block"
  m4 <- res_open 4
  case m4 of
    Nothing -> putStrLn "res_open 4: Nothing"
    Just r -> do
      live "in the alternative"
  live "after the alternative"
  nullable
  live "after nullable"
  order <- closedOrder 0
  putStrLn ("closed: " ++ unwords (map show order))
