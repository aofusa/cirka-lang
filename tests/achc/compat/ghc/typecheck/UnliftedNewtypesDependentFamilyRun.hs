-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/UnliftedNewtypesDependentFamilyRun.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE DataKinds #-}
{-# LANGUAGE GADTSyntax #-}
{-# LANGUAGE MagicHash #-}
{-# LANGUAGE PolyKinds #-}
{-# LANGUAGE TypeFamilies #-}
{-# LANGUAGE UnliftedNewtypes #-}

import GHC.Int (Int(I#))
import GHC.Word (Word(W#))
import GHC.Exts (Int#,Word#)
import Data.Proxy (Proxy(..))
import GHC.Exts (TYPE,RuntimeRep(..))

main :: IO ()
main = case method (Proxy :: Proxy 'IntRep) of
  BarIntC y -> case method (Proxy :: Proxy 'WordRep) of
    BarWordC z -> do
      print (I# y)
      print (W# z)

class Foo (a :: RuntimeRep) where
  data Bar a :: TYPE a
  method :: Proxy a -> Bar a

instance Foo 'IntRep where
  newtype instance Bar 'IntRep = BarIntC Int#
  method _ = BarIntC 5#

instance Foo 'WordRep where
  newtype instance Bar 'WordRep :: TYPE 'WordRep where
    BarWordC :: Word# -> Bar 'WordRep
  method _ = BarWordC 7##
