-- opt-levels: 2
-- max-allocs: 200
-- max-forced: 100
-- An action run on some iterations only (`when`, BUG-0035): the action
-- `print i` is used in several alternatives of the `case` on the
-- condition (`mod` takes the sign apart), and is built only in those,
-- not as a thunk in every iteration (100 000 allocations before).
module Main where

import Control.Monad (when)

loop :: Int -> Int -> IO Int
loop 0 acc = pure acc
loop i acc = do
  when (i `mod` 25000 == 0) (print i)
  loop (i - 1) (acc + i)

main :: IO ()
main = loop 100000 0 >>= print
