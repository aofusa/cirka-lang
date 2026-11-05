-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/T11049.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE ImplicitParams, RankNTypes #-}
import GHC.Stack

foo :: (?callStack :: CallStack) => [Int]
foo = map (srcLocStartLine . snd) (getCallStack ?callStack)

bar1 :: [Int]
bar1 = foo

bar2 :: [Int]
bar2 = let ?callStack = freezeCallStack ?callStack in foo

main :: IO ()
main = do
  print bar1
  print bar2
  withFrozenCallStack (error "look ma, no stack!")
