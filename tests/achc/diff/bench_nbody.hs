-- nbody-like 2D integration on Doubles with a fixed number of steps
module Main where

data Body = Body { bx, by, bvx, bvy, bm :: Double } deriving Show

accel :: [Body] -> Body -> (Double, Double)
accel bs b = foldl add (0, 0) [pull o | o <- bs, bx o /= bx b || by o /= by b]
  where
    add (a1, a2) (c1, c2) = (a1 + c1, a2 + c2)
    pull o =
      let dx = bx o - bx b
          dy = by o - by b
          d2 = dx * dx + dy * dy + 0.01
          d = sqrt d2
          f = bm o / (d2 * d)
      in (f * dx, f * dy)

step :: Double -> [Body] -> [Body]
step dt bs = map upd bs
  where
    upd b =
      let (ax, ay) = accel bs b
          vx' = bvx b + dt * ax
          vy' = bvy b + dt * ay
      in b { bx = bx b + dt * vx', by = by b + dt * vy', bvx = vx', bvy = vy' }

nsteps :: Int -> [Body] -> [Body]
nsteps 0 bs = bs
nsteps n bs = nsteps (n - 1) (step 0.01 bs)

kinetic :: [Body] -> Double
kinetic bs = sum [0.5 * bm b * (bvx b * bvx b + bvy b * bvy b) | b <- bs]

bodies0 :: [Body]
bodies0 =
  [ Body 0 0 0 0 10
  , Body 3 0 0 1.5 1
  , Body (-4) 0 0 (-1) 0.5
  ]

go :: Int -> [Body] -> IO ()
go k bs
  | k > 5 = return ()
  | otherwise = do
      putStrLn ("round " ++ show k)
      mapM_ (\b -> print (bx b, by b)) bs
      print (kinetic bs)
      go (k + 1) (nsteps 20 bs)

main :: IO ()
main = go 0 bodies0
