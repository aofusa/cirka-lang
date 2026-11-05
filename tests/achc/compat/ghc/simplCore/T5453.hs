-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T5453.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}
module Main where

import GHC.Exts

data Var = TyVar !Int Bool Bool
         | TcTyVar Bool !Int Bool
         | Var Bool Bool !Int
         deriving (Show)

scrut :: Var -> (Bool, String)
scrut v = (True, case v of
    TcTyVar {} -> "OK"
    _ -> show v ++ show (case (case v of
                                 TyVar b _ _ -> b
                                 Var _ _ b -> b) of
                           I# x# -> if isTrue# (x# ==# 7#)
                                    then show (I# (x# +# 1#))
                                    else show (I# (x# +# 2#))))

main = putStrLn $ snd (scrut (TcTyVar True 1 False))
