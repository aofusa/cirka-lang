-- Imported from GHC 9.4.7 testsuite/tests/programs/barton-mangler-bug/Bug.hs (BSD-3-Clause; see LICENSE in this directory).
module Bug where

data Eq a => Foo a = Foo { x :: a }

foo :: Foo Int
foo = Foo{}
