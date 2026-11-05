-- Imported from GHC 9.4.7 testsuite/tests/simplCore/should_run/T19413.hs (BSD-3-Clause; see LICENSE in this directory).
{-# OPTIONS_GHC -O1 #-}

import System.IO.Unsafe
import Data.IORef

{-# NOINLINE resourceId #-}
resourceId :: IO Int
resourceId = unsafePerformIO counter

counter :: IO (IO Int)
counter = do
    ref <- newIORef 0
    pure $ atomicModifyIORef' ref $ \i -> let j = i + 1 in (j, j)

main = do
    print =<< resourceId
    print =<< resourceId
