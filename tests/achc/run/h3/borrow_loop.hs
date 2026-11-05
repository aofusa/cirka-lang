{-# LANGUAGE LinearTypes #-}
{-# LANGUAGE RankNTypes #-}
-- ghc: skip
-- opt-levels: 2
-- max-allocs: 700000
-- max-array-copies: 0
-- A loop updating an unboxed array in place through `borrowMut` and the
-- `Borrow` monad (BUG-0011). The monad's dictionary (`Λr. Monad$Dict …`)
-- is resolved and IO's bind is inlined through its cast, so each step
-- allocates only the closures of the do block: 66 per step here, 87
-- before the fix.
module Main where

import Cirka.Ownership
import Cirka.Borrow
import qualified Cirka.Array.Unboxed as U

type Cells = Unique (U.UArray Double)

step :: MutRef r Cells -> Borrow r ()
step m = go 0
  where
    get k = U.index (asRef m) k
    go i
      | i >= 3 = return ()
      | otherwise = do
          x <- get i
          y <- get (i + 1)
          U.writeAt m i ((x + y) / 2)
          go (i + 1)

run :: Int -> Cells %1 -> Cells
run 0 c = c
run n c = case borrowMut c step of
  (c', ()) -> run (n - 1) c'

main :: IO ()
main = do
  let c = run 10000 (U.fromListU [0, 10, 20, 30])
  print (map truncate (U.toList (freeze c)) :: [Int])
