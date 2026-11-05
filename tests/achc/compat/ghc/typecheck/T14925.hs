-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T14925.hs (BSD-3-Clause; see LICENSE in this directory).
import Data.Typeable

data Ω = Ω

main :: IO ()
main = print $ typeOf Ω
