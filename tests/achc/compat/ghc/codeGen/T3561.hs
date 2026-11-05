-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/T3561.hs (BSD-3-Clause; see LICENSE in this directory).
main = print $ pqr' 0 1

pqr' :: Int -> Int -> Integer
pqr' a b | a == b - 1 = rab
         | otherwise = ram * rmb
    where m = (a + b) `div` 2
          ram = pqr' a m
          rmb = pqr' m b
          rab = toInteger (6 * b - 5) * toInteger (2 * b - 1) *
                toInteger (6 * b - 1)
