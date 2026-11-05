-- Imported from GHC 9.4.7 testsuite/tests/typecheck/should_run/TcTypeSymbolSimpleRun.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE DataKinds, TypeOperators, TypeFamilies #-}
module Main(main) where
import GHC.TypeLits
import Data.Proxy

--------------------------------------------------------------------------------
-- Test top-reactions

tappend :: Proxy (AppendSymbol x y) -> Proxy x -> Proxy y
tappend _ _ = Proxy

tappend2 :: Proxy (AppendSymbol x y) -> (Proxy x, Proxy y)
tappend2 _ = (Proxy, Proxy)

main :: IO ()
main = print [ symbolVal (tappend (Proxy :: Proxy "abc") (Proxy :: Proxy "ab"))
               == "c"
             , let (p, q) = tappend2 (Proxy :: Proxy "")
               in (symbolVal p, symbolVal q) == ("", "")
             ]
  where
  sh x = show (natVal x)
