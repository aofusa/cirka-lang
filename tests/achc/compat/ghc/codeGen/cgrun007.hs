-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/cgrun007.hs (BSD-3-Clause; see LICENSE in this directory).
data Tree a = Leaf a | Branch (Tree a) (Tree a)

main = print (height our_tree)
    where
      our_tree :: Tree Int
      our_tree =
        Branch (Branch (Leaf 1) (Branch (Branch (Leaf 1) (Leaf 1)) (Leaf 1)))
               (Branch (Leaf 1) (Leaf 1))


height :: Tree a -> Int

height (Leaf _)         = 1
height (Branch t1 t2)   = 1 + max (height t1) (height t2)
