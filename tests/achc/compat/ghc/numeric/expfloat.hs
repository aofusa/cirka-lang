-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/expfloat.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash #-}
-- tests that expFloat# works (had linking problems on Windows)

import GHC.Exts

main = do
  print (F# (expFloat# 3.45#))
