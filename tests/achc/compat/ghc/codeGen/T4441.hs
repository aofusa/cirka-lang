-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/T4441.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

import Numeric
import System.IO

main = do
 let d = read "2.0e-2" :: Double
 print $ "Float Version : " ++ (fToStr $ realToFrac d)
 print $ "Double Version: " ++ (dToStr d)

double :: IO Double
double = do
   x <- getLine
   return $ read x

dToStr :: Double -> String
dToStr d = show d

fToStr :: Float -> String
fToStr = (dToStr . realToFrac)
