-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T11120.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE MagicHash, DataKinds #-}
-- See also TypeOf.hs

import GHC.Exts
import Data.Typeable

data CharHash = CharHash Char#

main :: IO ()
main = print $ typeRep (Proxy :: Proxy 'CharHash)
