module Main (main) where

import Text.Greet (greet)

main :: IO ()
main = putStrLn (greet "package")
