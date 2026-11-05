-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun046.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

import System.IO

-- !!! CAF space leaks

main = lots_of_xs 10000

lots_of_xs 0 = return ()
lots_of_xs n = putChar 'x' >> lots_of_xs (n-1)
