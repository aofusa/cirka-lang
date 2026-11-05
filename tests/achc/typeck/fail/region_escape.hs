{-# LANGUAGE RankNTypes #-}
-- Escaping regions (FEAT-0127, E0320): neither the arena, a handle into it
-- nor a computation of the region can leave `withRegion`.
module Main where

import Cirka.Region

leakArena :: Arena r
leakArena = withRegion (\a -> pureR a)

leakHandle :: In r Int
leakHandle = withRegion (\a -> allocIn a 1)

leakComp :: RIO r (In r Int)
leakComp = withRegion (\a -> pureR (allocIn a 1))

-- fine: a result without the region
ok :: Int
ok = withRegion (\a -> pureR 1)
