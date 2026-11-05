-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/T4136.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

data T = (:=:) {- | (:!=:) -} deriving (Show,Read)

main
 = do  putStrLn ("show (:=:) = " ++ show (:=:))
       putStrLn ("read (show (:=:)) :: T = " ++
                  show (read (show (:=:)) :: T))

