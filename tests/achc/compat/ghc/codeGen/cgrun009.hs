-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun009.hs (BSD-3-Clause; see LICENSE in this directory).
main = print (length take_list)
 where
    take_list :: [Int]
    take_list = takeWhile (\ x -> x < 6) given_list

    given_list :: [Int]
    given_list = [1,2,3,4,5,6,7,8,9]
