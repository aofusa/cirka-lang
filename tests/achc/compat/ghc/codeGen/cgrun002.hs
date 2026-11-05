-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun002.hs (BSD-3-Clause; see LICENSE in this directory).
main = print ((f id2) (10 + thirty_two))
  where
    f x = g x
      where
        g x = h x
          where
            h x = x

    thirty_two :: Int
    thirty_two = 32

id2 x = x
