-- Data.Bits operations on Int, Word8 and Integer
module Main where

import Data.Bits
import Data.Word

bitsOf :: Int -> String
bitsOf n = [if testBit n i then '1' else '0' | i <- [7, 6 .. 0]]

parity :: Int -> Bool
parity n = odd (popCount n)

gray :: Int -> Int
gray n = n `xor` (n `shiftR` 1)

reverse8 :: Int -> Int
reverse8 n = sum [bit (7 - i) | i <- [0 .. 7], testBit n i]

main :: IO ()
main = do
  print ((12 :: Int) .&. 10, (12 :: Int) .|. 10, xor (12 :: Int) 10)
  print (shiftL (1 :: Int) 10, shiftR (1024 :: Int) 3)
  print (shiftL (1 :: Int) 63)
  print (shiftR (-16 :: Int) 2, shiftR (-1 :: Int) 10)
  print (complement (0 :: Int), complement (5 :: Int))
  print (complement (5 :: Word8), complement (0 :: Word8))
  print (popCount (255 :: Int), popCount (-1 :: Int), popCount (0 :: Int))
  print (popCount (2 ^ 100 - 1 :: Integer))
  print (map (testBit (10 :: Int)) [0 .. 4])
  print (setBit (0 :: Int) 5, clearBit (255 :: Int) 0, bit 4 :: Int)
  print (setBit (0 :: Word8) 7)
  print (shiftL (200 :: Word8) 1, shiftR (200 :: Word8) 3)
  print ((0xFF :: Int) .&. 0x0F, (0xF0 :: Int) .|. 0x0F)
  print (map bitsOf [0, 1, 5, 128, 255])
  print (map gray [0 .. 8])
  print (map parity [0 .. 8])
  print (map reverse8 [1, 2, 3, 6])
  print (shiftL (1 :: Integer) 100)
  print ((2 ^ 70 + 5 :: Integer) .&. 7, xor (2 ^ 70 :: Integer) (2 ^ 70 + 1))
  print (foldr (\b acc -> acc * 2 + (if b then 1 else 0)) (0 :: Int) [True, False, True, True])
  print (filter (\n -> n .&. (n - 1) == 0) [1 .. 64 :: Int])
