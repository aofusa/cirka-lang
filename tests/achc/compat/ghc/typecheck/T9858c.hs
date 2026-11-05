-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T9858c.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE KindSignatures #-}
module Main(main) where

import Data.Typeable
import Data.Kind

test1 :: Bool
test1 = typeRep (Proxy :: Proxy (() :: Type)) ==
        typeRep (Proxy :: Proxy (() :: Constraint))

test2 :: Bool
test2 = typeRepTyCon (typeRep (Proxy :: Proxy (Int,Int))) ==
        typeRepTyCon (typeRep (Proxy :: Proxy (Eq Int, Eq Int)))

main :: IO ()
main = print (test1,test2)



