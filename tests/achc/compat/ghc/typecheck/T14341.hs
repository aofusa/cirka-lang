-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T14341.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE TypeApplications #-}

import Type.Reflection

main :: IO ()
main = do
    print $ typeRep @((,,))
    print $ typeRep @((,,) Int)
    print $ typeRep @((,,) Int Int Int)
