-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun008.hs (BSD-3-Clause; see LICENSE in this directory).
main = print (length comp_list)
 where
    comp_list :: [(Int,Int)]
    comp_list = [ (elem1,elem2)
                | elem1 <- given_list,
                  elem2 <- given_list,
                  elem1 >= (4::Int),
                  elem2 <  (3::Int)
                ]

    given_list :: [Int]
    given_list = [1,2,3,4,5,6,7,8,9]
