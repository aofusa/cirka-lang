-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/numrun010.hs (BSD-3-Clause; see LICENSE in this directory).
-- !!! tests that local fixity declarations work

-- If local fixity decls don't work you get "14"
-- The right answer is "11"

val = 3 +! 4 *! 2
    where (+!) = (+)
          (*!) = (*)
          infixl 6 +!
          infixl 7 *!

main = print val

