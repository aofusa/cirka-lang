-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun063.hs (BSD-3-Clause; see LICENSE in this directory).

{-
Check that we aren't making gcc misinterpret our strings as trigraphs.
#2968.
http://gcc.gnu.org/onlinedocs/cpp/Initial-processing.html
-}

module Main where

main :: IO ()
main = do putStrLn "??("
          putStrLn "??)"
          putStrLn "??<"
          putStrLn "??>"
          putStrLn "??="
          putStrLn "??/"
          putStrLn "??'"
          putStrLn "??!"
          putStrLn "??-"

