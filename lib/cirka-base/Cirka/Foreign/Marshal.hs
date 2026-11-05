{-# LANGUAGE NoImplicitPrelude #-}
-- | Memory allocation, arrays and C strings (FEAT-0116; base's
-- Foreign.Marshal.* and Foreign.C.String). Memory comes from the C heap
-- (`malloc` / `free`) and is not tracked by the reference counts. Strings
-- are converted to and from UTF-8.
module Cirka.Foreign.Marshal (module Cirka.Foreign.Marshal) where

import Cirka.Base
import Cirka.Foreign
import Cirka.Foreign.Types
import Data.Char (chr, ord)

foreign import ccall unsafe "stdlib.h malloc" cirkaCMalloc :: Int -> IO (Ptr a)
foreign import ccall unsafe "stdlib.h calloc" cirkaCCalloc :: Int -> Int -> IO (Ptr a)
foreign import ccall unsafe "stdlib.h realloc" cirkaCRealloc :: Ptr a -> Int -> IO (Ptr b)
foreign import ccall unsafe "stdlib.h free" free :: Ptr a -> IO ()
foreign import ccall unsafe "string.h memcpy" cirkaCMemcpy :: Ptr a -> Ptr b -> Int -> IO (Ptr a)
foreign import ccall unsafe "string.h memmove" cirkaCMemmove :: Ptr a -> Ptr b -> Int -> IO (Ptr a)
foreign import ccall unsafe "string.h memset" cirkaCMemset :: Ptr a -> CInt -> Int -> IO (Ptr a)

-- ---- allocation (Foreign.Marshal.Alloc) ----

cirkaCheckAlloc :: [Char] -> Ptr a -> IO (Ptr a)
cirkaCheckAlloc what p =
  if p == nullPtr then ioError (userError (what ++ ": out of memory")) else return p

mallocBytes :: Int -> IO (Ptr a)
mallocBytes n = cirkaCMalloc (max n 1) >>= cirkaCheckAlloc "malloc"

callocBytes :: Int -> IO (Ptr a)
callocBytes n = cirkaCCalloc 1 (max n 1) >>= cirkaCheckAlloc "calloc"

reallocBytes :: Ptr a -> Int -> IO (Ptr a)
reallocBytes p n = cirkaCRealloc p (max n 1) >>= cirkaCheckAlloc "realloc"

-- | A stand-in element that fixes the type of `sizeOf` (never evaluated:
-- `sizeOf` and `alignment` do not look at their argument).
cirkaMarshalElem :: a
cirkaMarshalElem = errorWithoutStackTrace "Foreign.Marshal: the element was evaluated"

cirkaSizedAlloc :: Storable a => (Int -> IO (Ptr a)) -> Int -> a -> IO (Ptr a)
cirkaSizedAlloc f n x = f (n * sizeOf x)

malloc :: Storable a => IO (Ptr a)
malloc = cirkaSizedAlloc mallocBytes 1 cirkaMarshalElem

calloc :: Storable a => IO (Ptr a)
calloc = cirkaSizedAlloc callocBytes 1 cirkaMarshalElem

realloc :: Storable b => Ptr a -> IO (Ptr b)
realloc p = cirkaSizedAlloc (reallocBytes (castPtr p)) 1 cirkaMarshalElem

alloca :: Storable a => (Ptr a -> IO b) -> IO b
alloca f = cirkaAllocaFor (cirkaFunElem f) f

cirkaFunElem :: (Ptr a -> IO b) -> a
cirkaFunElem _ = errorWithoutStackTrace "Foreign.Marshal: the element was evaluated"

cirkaAllocaFor :: Storable a => a -> (Ptr a -> IO b) -> IO b
cirkaAllocaFor x = allocaBytes (sizeOf x)

-- | The memory is freed when the action returns (exceptions are not
-- caught: an uncaught exception ends the program).
allocaBytes :: Int -> (Ptr a -> IO b) -> IO b
allocaBytes n f = do
  p <- mallocBytes n
  r <- f p
  free p
  return r

allocaBytesAligned :: Int -> Int -> (Ptr a -> IO b) -> IO b
allocaBytesAligned n _ = allocaBytes n

-- ---- utilities (Foreign.Marshal.Utils) ----

with :: Storable a => a -> (Ptr a -> IO b) -> IO b
with x f = alloca (\p -> poke p x >> f p)

new :: Storable a => a -> IO (Ptr a)
new x = do
  p <- mallocBytes (sizeOf x)
  poke p x
  return p

fromBool :: Num a => Bool -> a
fromBool b = if b then 1 else 0

toBool :: (Eq a, Num a) => a -> Bool
toBool x = x /= 0

maybeWith :: (a -> (Ptr b -> IO c) -> IO c) -> Maybe a -> (Ptr b -> IO c) -> IO c
maybeWith _ Nothing f = f nullPtr
maybeWith w (Just x) f = w x f

maybePeek :: (Ptr a -> IO b) -> Ptr a -> IO (Maybe b)
maybePeek pk p = if p == nullPtr then return Nothing else fmap Just (pk p)

copyBytes :: Ptr a -> Ptr a -> Int -> IO ()
copyBytes d s n = cirkaCMemcpy d s n >> return ()

moveBytes :: Ptr a -> Ptr a -> Int -> IO ()
moveBytes d s n = cirkaCMemmove d s n >> return ()

fillBytes :: Ptr a -> Word8 -> Int -> IO ()
fillBytes p b n = cirkaCMemset p (fromIntegral b) n >> return ()

-- ---- arrays (Foreign.Marshal.Array) ----

mallocArray :: Storable a => Int -> IO (Ptr a)
mallocArray n = cirkaSizedAlloc mallocBytes n cirkaMarshalElem

mallocArray0 :: Storable a => Int -> IO (Ptr a)
mallocArray0 n = mallocArray (n + 1)

callocArray :: Storable a => Int -> IO (Ptr a)
callocArray n = cirkaSizedAlloc callocBytes n cirkaMarshalElem

reallocArray :: Storable a => Ptr a -> Int -> IO (Ptr a)
reallocArray p n = cirkaSizedAlloc (reallocBytes p) n cirkaMarshalElem

allocaArray :: Storable a => Int -> (Ptr a -> IO b) -> IO b
allocaArray n f = allocaBytes (n * sizeOf (cirkaFunElem f)) f

allocaArray0 :: Storable a => Int -> (Ptr a -> IO b) -> IO b
allocaArray0 n = allocaArray (n + 1)

peekArray :: Storable a => Int -> Ptr a -> IO [a]
peekArray n p
  | n <= 0 = return []
  | otherwise = go (n - 1) []
  where
    go i acc
      | i < 0 = return acc
      | otherwise = do
          x <- peekElemOff p i
          go (i - 1) (x : acc)

peekArray0 :: (Storable a, Eq a) => a -> Ptr a -> IO [a]
peekArray0 marker p = do
  n <- lengthArray0 marker p
  peekArray n p

pokeArray :: Storable a => Ptr a -> [a] -> IO ()
pokeArray p xs = go 0 xs
  where
    go _ [] = return ()
    go i (y : ys) = pokeElemOff p i y >> go (i + 1) ys

pokeArray0 :: Storable a => a -> Ptr a -> [a] -> IO ()
pokeArray0 marker p xs = do
  pokeArray p xs
  pokeElemOff p (length xs) marker

newArray :: Storable a => [a] -> IO (Ptr a)
newArray xs = do
  p <- mallocArray (length xs)
  pokeArray p xs
  return p

newArray0 :: Storable a => a -> [a] -> IO (Ptr a)
newArray0 marker xs = do
  p <- mallocArray0 (length xs)
  pokeArray0 marker p xs
  return p

withArray :: Storable a => [a] -> (Ptr a -> IO b) -> IO b
withArray xs f = withArrayLen xs (\_ p -> f p)

withArray0 :: Storable a => a -> [a] -> (Ptr a -> IO b) -> IO b
withArray0 marker xs f = withArrayLen0 marker xs (\_ p -> f p)

withArrayLen :: Storable a => [a] -> (Int -> Ptr a -> IO b) -> IO b
withArrayLen xs f =
  let n = length xs
  in allocaArray n (\p -> pokeArray p xs >> f n p)

withArrayLen0 :: Storable a => a -> [a] -> (Int -> Ptr a -> IO b) -> IO b
withArrayLen0 marker xs f =
  let n = length xs
  in allocaArray0 n (\p -> pokeArray0 marker p xs >> f n p)

copyArray :: Storable a => Ptr a -> Ptr a -> Int -> IO ()
copyArray d s n = copyBytes d s (n * sizeOf (cirkaPtrElem d))

moveArray :: Storable a => Ptr a -> Ptr a -> Int -> IO ()
moveArray d s n = moveBytes d s (n * sizeOf (cirkaPtrElem d))

lengthArray0 :: (Storable a, Eq a) => a -> Ptr a -> IO Int
lengthArray0 marker p = go 0
  where
    go i = do
      x <- peekElemOff p i
      if x == marker then return i else go (i + 1)

advancePtr :: Storable a => Ptr a -> Int -> Ptr a
advancePtr p n = plusPtr p (n * sizeOf (cirkaPtrElem p))

-- ---- C strings (Foreign.C.String), UTF-8 ----

type CString = Ptr CChar

type CStringLen = (Ptr CChar, Int)

castCharToCChar :: Char -> CChar
castCharToCChar c = fromIntegral (ord c)

castCCharToChar :: CChar -> Char
castCCharToChar c = chr (fromIntegral (fromIntegral c :: Word8))

castCharToCUChar :: Char -> CUChar
castCharToCUChar c = fromIntegral (ord c)

castCUCharToChar :: CUChar -> Char
castCUCharToChar c = chr (fromIntegral c)

-- | The UTF-8 bytes of a string.
cirkaEncodeUtf8 :: [Char] -> [Int]
cirkaEncodeUtf8 [] = []
cirkaEncodeUtf8 (c : cs)
  | n < 0x80 = n : rest
  | n < 0x800 = (0xC0 + shiftR n 6) : cont n 0 : rest
  | n < 0x10000 = (0xE0 + shiftR n 12) : cont n 6 : cont n 0 : rest
  | otherwise = (0xF0 + shiftR n 18) : cont n 12 : cont n 6 : cont n 0 : rest
  where
    n = ord c
    rest = cirkaEncodeUtf8 cs
    cont x s = 0x80 + (shiftR x s .&. 0x3F)

-- | Decodes UTF-8 (an invalid byte becomes U+FFFD).
cirkaDecodeUtf8 :: [Int] -> [Char]
cirkaDecodeUtf8 [] = []
cirkaDecodeUtf8 (b : bs)
  | b < 0x80 = chr b : cirkaDecodeUtf8 bs
  | b >= 0xC0 && b < 0xE0 = multi 1 (b .&. 0x1F)
  | b >= 0xE0 && b < 0xF0 = multi 2 (b .&. 0x0F)
  | b >= 0xF0 && b < 0xF8 = multi 3 (b .&. 0x07)
  | otherwise = '\xFFFD' : cirkaDecodeUtf8 bs
  where
    multi k acc =
      let (cs, rest) = splitAt k bs
      in if length cs == k && all (\x -> x .&. 0xC0 == 0x80) cs
           then chr (foldl (\a x -> shiftL a 6 + (x .&. 0x3F)) acc cs) : cirkaDecodeUtf8 rest
           else '\xFFFD' : cirkaDecodeUtf8 bs

newCString :: [Char] -> IO CString
newCString s = newArray0 0 (map fromIntegral (cirkaEncodeUtf8 s))

newCStringLen :: [Char] -> IO CStringLen
newCStringLen s = do
  let bytes = map fromIntegral (cirkaEncodeUtf8 s)
  p <- newArray bytes
  return (p, length bytes)

withCString :: [Char] -> (CString -> IO a) -> IO a
withCString s = withArray0 0 (map fromIntegral (cirkaEncodeUtf8 s))

withCStringLen :: [Char] -> (CStringLen -> IO a) -> IO a
withCStringLen s f = withArrayLen (map fromIntegral (cirkaEncodeUtf8 s)) (\n p -> f (p, n))

peekCString :: CString -> IO [Char]
peekCString p = do
  bytes <- peekArray0 0 (castPtr p :: Ptr Word8)
  return (cirkaDecodeUtf8 (map fromIntegral bytes))

peekCStringLen :: CStringLen -> IO [Char]
peekCStringLen (p, n) = do
  bytes <- peekArray n (castPtr p :: Ptr Word8)
  return (cirkaDecodeUtf8 (map fromIntegral bytes))

newCAString :: [Char] -> IO CString
newCAString s = newArray0 0 (map castCharToCChar s)

peekCAString :: CString -> IO [Char]
peekCAString p = fmap (map castCCharToChar) (peekArray0 0 p)

withCAString :: [Char] -> (CString -> IO a) -> IO a
withCAString s = withArray0 0 (map castCharToCChar s)
