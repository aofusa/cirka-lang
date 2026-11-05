-- Prelude functions not covered by classes.hs, and reading stdin.
module Main where

main :: IO ()
main = do
  n <- readLn :: IO Int
  name <- getLine
  rest <- getContents
  putStrLn ("n=" ++ show n ++ " name=" ++ name)
  print (lines rest, words "  a  b\tc\n", unlines ["x", "y"], unwords ["p", "q"])
  print (until (> 100) (* 2) 1, asTypeOf 3 (4 :: Int), take 5 (iterate (* 3) 1), take 7 (cycle [1, 2, 3]))
  print (scanl (+) 0 [1, 2, 3], scanl1 max [3, 1, 4], scanr (+) 0 [1, 2, 3], scanr1 (+) [1, 2, 3])
  print (span even [2, 4, 5, 6], break (> 2) [1, 2, 3], splitAt 2 "hello", dropWhile (< 3) [1 .. 5])
  print (last [1, 2, 3], init [1, 2, 3], [10, 20, 30] !! 1, replicate 3 'x', reverse "abc", null [], notElem 2 [1, 3])
  print (unzip [(1, 'a'), (2, 'b')], unzip3 [(1, 'a', True)], zipWith (+) [1, 2] [10, 20, 30])
  print (either show (map succ) (Right "ab" :: Either Int String), maybe 0 (* 2) (Just 4), curry fst 1 2, uncurry (+) (3, 4))
  print (showString "ab" "cd", showParen True (shows 5) "", showsPrec 7 (-3 :: Int) "", show (Just ""))
  print (reads "12 rest" :: [(Int, String)], lex " foo bar", (read "(1, 'c', \"s\")" :: (Int, Char, String)))
  print (properFraction (3.75 :: Double) :: (Int, Double), ceiling 2.1, floor (-2.1), truncate (-2.9), round (-2.5))
  print (logBase 2 1024, atan2 1 1, isNaN (0 / 0), isInfinite (1 / 0), realToFrac (1.5 :: Double) :: Double, 2 ^^ (-2))
  print (minBound :: Int, maxBound :: Int, maxBound :: Char, fromEnum 'z', toEnum 65 :: Char, ['a' .. 'e'], [10, 8 .. 1])
  print (quotRem (-7) 2, divMod (-7) 2, toInteger (5 :: Int), fromIntegral (7 :: Int) * 0.5, signum (-3), abs (-3))
  print (sum [], product [1 .. 6], and [], or [True, False], all even [2, 4], any odd [2, 4], concat [[1], [2]])
  print (seq 1 2, id $! 3, const 1 undefined, fst (1, undefined), length [undefined, undefined])
  print (mconcat ["ab", "cd"], mempty :: String, [1, 2] <> [3], compare "a" "b" <> EQ, Just [1] <> Nothing <> Just [2])
  print (sequence_ [Just 1, Just 2], mapM_ Just [1, 2], (=<<) (\x -> [x, x]) [1, 2], [1, 2] >> "ab", fmap length (Right "abc" :: Either () String))
  print (lookup 'b' (zip "abc" [1 ..]), elem 'q' "quick", filter odd [1 .. 9], takeWhile (/= ' ') "hi there")
  print (gcd 0 0, lcm 0 5, subtract 1 0, negate 0.0, recip 4)
  print (pi / 4, sqrt 16, exp 0, sin 0, cos 0)
  print (show (1 / 3), show 1.0e7, show 9999999.0, show 0.1e-3, show 123.456, show (fromIntegral (maxBound :: Int) :: Double))
  print (read "  42" :: Int, read "-7" :: Int, read "3.0" :: Double, read "1e3" :: Double, read "'x'" :: Char, read "\"a\\nb\"" :: String)
