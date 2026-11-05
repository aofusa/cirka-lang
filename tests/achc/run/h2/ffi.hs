-- Haskell 2010 FFI: foreign import ccall, Ptr, Storable, marshalling,
-- sized integers and Data.Bits (FEAT-0116).
import Data.Bits
import Data.Int
import Data.Word
import Foreign
import Foreign.C

foreign import ccall "math.h cos" c_cos :: Double -> Double
foreign import ccall unsafe "stdlib.h labs" c_labs :: CLong -> CLong
foreign import ccall "stdlib.h abs" c_abs :: CInt -> CInt
foreign import ccall "math.h ldexpf" c_ldexpf :: CFloat -> CInt -> CFloat
foreign import ccall "math.h frexp" c_frexp :: CDouble -> Ptr CInt -> IO CDouble
foreign import ccall "string.h strlen" c_strlen :: CString -> IO CSize
foreign import ccall "ctype.h toupper" c_toupper :: CInt -> CInt
foreign import ccall "stdlib.h qsort" c_qsort_unused :: Ptr () -> CSize -> CSize -> FunPtr () -> IO ()
foreign import ccall "string.h memcmp" c_memcmp :: Ptr a -> Ptr a -> CSize -> IO CInt

newtype Meters = Meters CDouble deriving (Show)

foreign import ccall "math.h fabs" c_fabs :: Meters -> Meters

main :: IO ()
main = do
  print (c_cos 0, c_labs (-42), c_abs (-7), c_ldexpf 1.5 3, c_fabs (Meters (-2.5)))
  print (map (toEnum . fromIntegral . c_toupper . fromIntegral . fromEnum) "shout" :: String)
  (m, e) <- alloca $ \p -> do
    m <- c_frexp 12.0 p
    e <- peek p
    return (m, e)
  print (m, e)
  n <- withCString "h\233llo" c_strlen
  s <- withCString "\26085\26412\35486 ok" peekCString
  print (n, s, length s)
  p <- newArray [10, 20, 30 :: Int32]
  xs <- peekArray 3 p
  pokeElemOff p 1 (-5)
  ys <- peekArray 3 p
  free p
  print (xs, ys, sum ys)
  ds <- withArrayLen [1.5, 2.25 :: Double] $ \len q -> do
    a <- peekElemOff q 1
    poke q 9.0
    b <- peek q
    return (len, a, b)
  print ds
  r <- allocaBytes 16 $ \q -> do
    poke (castPtr q) (0x0102030405060708 :: Word64)
    b0 <- peekByteOff q 0 :: IO Word8
    b7 <- peekByteOff q 7 :: IO Word8
    pokeByteOff q 8 (1.5 :: Float)
    f <- peekByteOff q 8 :: IO Float
    c <- withArray [1, 2, 3 :: Word8] $ \a -> withArray [1, 2, 4 :: Word8] $ \b -> c_memcmp a b 3
    return (b0, b7, f, signum c)
  print r
  print (nullPtr :: Ptr Int, nullPtr `plusPtr` 255 :: Ptr (), minusPtr (nullPtr `plusPtr` 24) (nullPtr :: Ptr ()))
  print (sizeOf (0 :: CInt), sizeOf (0 :: Double), sizeOf (0 :: CChar), alignment (0 :: Int16), sizeOf nullPtr)
  print ((255 :: Word8) + 1, (127 :: Int8) + 1, negate (1 :: Word32), fromIntegral (300 :: Int) :: Word8, fromIntegral (-1 :: Int) :: Word16)
  print (maxBound :: Int32, minBound :: Int16, maxBound :: Word64, minBound :: Int64, maxBound :: CUInt)
  print (div (-7) 2 :: Int8, quot (-7) 2 :: Int16, (200 :: Word8) * 2, toInteger (maxBound :: Word32) + 1)
  print (popCount (255 :: Int), popCount (-1 :: Int8), shiftL (1 :: Int) 10, shiftR (-16 :: Int) 2, shiftR (0xF0 :: Word8) 4)
  print (complement (0 :: Word16), rotate (0x81 :: Word8) 1, rotateR (1 :: Word32) 1, rotate (1 :: Int) (-1), xor (5 :: Int) 3)
  print (testBit (5 :: Int) 2, testBit (5 :: Int) 1, setBit (0 :: Int64) 63, clearBit (15 :: CInt) 0, bit 3 :: Word)
  print (countLeadingZeros (1 :: Word16), countTrailingZeros (8 :: Int32), finiteBitSize (0 :: CShort), shiftL (1 :: Int) 64)
  print ((.&.) (12 :: Word) 10, (.|.) (12 :: Int) 3, isSigned (0 :: Word8), isSigned (0 :: CChar), bitSizeMaybe (0 :: Int))
  print (read "200" :: Word8, read "-3" :: Int8, [1 .. 5 :: Word16], [minBound .. -126 :: Int8])
  print (castCharToCChar 'A', castCCharToChar 66, fromBool True :: CInt, toBool (0 :: CInt))
