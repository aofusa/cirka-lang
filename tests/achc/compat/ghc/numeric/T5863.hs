-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T5863.hs (BSD-3-Clause; see LICENSE in this directory).
import Text.Printf

main :: IO ()
main = do
  putStrLn $ printf "%.1f" (0.45 :: Double)
  putStrLn $ printf "%.1f" (0.55 :: Double)
