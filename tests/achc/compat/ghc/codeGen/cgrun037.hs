-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun037.hs (BSD-3-Clause; see LICENSE in this directory).
-- Andy Gill bug report 95/08:
-- Constant strings with '\0' in them don't work :-
--
main = putStrLn "hello\0 world"
--main = putStrLn "hello0 world"

