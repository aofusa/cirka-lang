-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/UnliftedNewtypesCoerceRun.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE GADTSyntax #-}
{-# LANGUAGE KindSignatures #-}
{-# LANGUAGE MagicHash #-}
{-# LANGUAGE UnliftedNewtypes #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE PolyKinds #-}
{-# LANGUAGE UnboxedTuples #-}
{-# LANGUAGE UnboxedSums #-}
{-# LANGUAGE TypeInType #-}
{-# LANGUAGE TypeFamilies #-}

import GHC.Int (Int(I#))
import GHC.Word (Word(W#))
import GHC.Exts (Int#,Word#,(+#))
import GHC.Types
import Data.Coerce (coerce)

main :: IO ()
main = do
  print (I# (coerce (Foo 5#)))

newtype Foo = Foo Int#
