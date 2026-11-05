-- Imported from GHC 9.4.7 testsuite/tests/codeGen/should_run/T9001.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE RankNTypes #-}

newtype FMList = FM {unFM :: forall m. m -> m}

main = print (delete 2000 (FM id) :: Int)

delete 0 _ = 0
delete n (FM a) = a $ delete (n-1) $ FM $ \g -> a (const g) undefined
