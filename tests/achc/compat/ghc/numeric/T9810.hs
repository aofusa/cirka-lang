-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T9810.hs (BSD-3-Clause; see LICENSE in this directory).
main = do
    -- NOTE: the `abs` is to compensate for WAY=optllvm
    --       having a positive sign for 0/0

    putStrLn "## Double ##"
    print $ idRational ( 1/0 :: Double)
    print $ idRational (-1/0 :: Double)
    print $ abs $ idRational ( 0/0 :: Double)
    print $ idReencode ( 1/0 :: Double)
    print $ idReencode (-1/0 :: Double)
    print $ abs $ idReencode ( 0/0 :: Double)

    putStrLn "## Float ##"
    print $ idRational ( 1/0 :: Float)
    print $ idRational (-1/0 :: Float)
    print $ abs $ idRational ( 0/0 :: Float)
    print $ idReencode ( 1/0 :: Float)
    print $ idReencode (-1/0 :: Float)
    print $ abs $ idReencode ( 0/0 :: Float)
  where
    idRational :: (Real a, Fractional a) => a -> a
    idRational = fromRational . toRational

    idReencode :: (RealFloat a) => a -> a
    idReencode = uncurry encodeFloat . decodeFloat
