-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/tcrun037.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE ConstrainedClassMethods #-}

module Main where

class C a where
  op :: (Show a, Show b) => a -> b -> String
        -- This class op has local quantification, but
        -- also adds a constraint on 'a'

instance C Bool where
  op x y = show x ++ " " ++ show y

main = do { putStrLn (op True 'x'); putStrLn (op False (3::Int)) }
