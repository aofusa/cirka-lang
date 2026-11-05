-- Int8/Int16/Int32/Word8/Word16/Word wraparound via Data.Int and Data.Word
module Main where

import Data.Int
import Data.Word

main :: IO ()
main = do
  print (minBound :: Int8, maxBound :: Int8)
  print (minBound :: Int16, maxBound :: Int16)
  print (minBound :: Int32, maxBound :: Int32)
  print (maxBound :: Word8, maxBound :: Word16, maxBound :: Word32)
  print (maxBound :: Word)
  print ((127 :: Int8) + 1)
  print ((-128 :: Int8) - 1)
  print (negate (minBound :: Int8))
  print ((0 :: Word8) - 1)
  print ((200 :: Word8) + 100)
  print ((16 :: Word8) * 17)
  print (fromIntegral (300 :: Int) :: Word8)
  print (fromIntegral (-1 :: Int8) :: Word8)
  print (fromIntegral (200 :: Word8) :: Int8)
  print (fromIntegral (-1 :: Int) :: Word)
  print (fromIntegral (3000000000 :: Integer) :: Int32)
  print (fromIntegral (70000 :: Int) :: Word16)
  print ((0 :: Word) - 1)
  print (div (-128 :: Int8) 3, mod (-128 :: Int8) 3, quot (-128 :: Int8) 3, rem (-128 :: Int8) 3)
  print (div (250 :: Word8) 7, mod (250 :: Word8) 7)
  print (Just (-128 :: Int8), [minBound :: Int16])
  print (toInteger (maxBound :: Word) + 1)
  print [250 :: Word8 ..]
  print (map fromIntegral [100, 200, 300 :: Int] :: [Word8])
  print (sum (map fromIntegral [100, 100, 100 :: Int]) :: Word8)
  print ((maxBound :: Int32) + 1 == minBound)
  print (compare (255 :: Word8) 0, compare (-1 :: Int8) 0)
