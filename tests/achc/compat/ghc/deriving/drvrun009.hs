-- Imported from GHC 9.4.7 testsuite/tests/deriving/should_run/drvrun009.hs (BSD-3-Clause; see LICENSE in this directory).
-- !!! Check the Read instance for Array
-- [Not strictly a 'deriving' issue]

module Main( main ) where
import Data.Array

bds :: ((Int,Int),(Int,Int))
bds = ((1,4),(2,5))

type MyArr = Array (Int,Int) Int

a :: MyArr
a = array bds [ ((i,j), i+j) | (i,j) <- range bds ]

main = do { putStrLn (show a) ;
            let { b :: MyArr ;
                  b = read (show a) } ;
            putStrLn (show b)
          }

