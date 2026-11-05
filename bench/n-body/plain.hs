-- n-body (Benchmarks Game), single-threaded plain Haskell: aCHC and the
-- GHC -O2 baseline. Immutable bodies with strict fields, rebuilt every step
-- (the algorithm of main.cirka). Same output as rust/main.rs.
module Main where

import System.Environment (getArgs)

-- position, velocity, mass
data Body = B !Double !Double !Double !Double !Double !Double !Double

solarMass, daysPerYear :: Double
solarMass = 4 * pi * pi
daysPerYear = 365.24

planet :: Double -> Double -> Double -> Double -> Double -> Double -> Double -> Body
planet x y z vx vy vz m = B x y z (vx * daysPerYear) (vy * daysPerYear) (vz * daysPerYear) (m * solarMass)

initial :: [Body]
initial =
  [ B 0 0 0 0 0 0 solarMass
  , planet 4.84143144246472090 (-1.16032004402742839) (-0.103622044471123109)
      0.00166007664274403694 0.00769901118419740425 (-0.0000690460016972063023)
      0.000954791938424326609
  , planet 8.34336671824457987 4.12479856412430479 (-0.403523417114321381)
      (-0.00276742510726862411) 0.00499852801234917238 0.0000230417297573763929
      0.000285885980666130812
  , planet 12.8943695621391310 (-15.1111514016986312) (-0.223307578892655734)
      0.00296460137564761618 0.00237847173959480950 (-0.0000296589568540237556)
      0.0000436624404335156298
  , planet 15.3796971148509165 (-25.9193146099879641) 0.179258772950371181
      0.00268067772490389322 0.00162824170038242295 (-0.0000951592254519715870)
      0.0000515138902046611451
  ]

offset :: [Body] -> [Body]
offset [] = []
offset bs@(B x y z _ _ _ m : rest) =
  let (px, py, pz) = foldl (\(a, b, c) (B _ _ _ vx vy vz mm) -> (a + vx * mm, b + vy * mm, c + vz * mm)) (0, 0, 0) bs
   in B x y z (negate px / solarMass) (negate py / solarMass) (negate pz / solarMass) m : rest

energy :: [Body] -> Double -> Double
energy [] e = e
energy (b@(B _ _ _ vx vy vz m) : bs) e =
  let k = e + 0.5 * m * (vx * vx + vy * vy + vz * vz)
   in energy bs (potential b bs k)

potential :: Body -> [Body] -> Double -> Double
potential _ [] e = e
potential b@(B x y z _ _ _ m) (B x2 y2 z2 _ _ _ m2 : bs) e =
  let dx = x - x2
      dy = y - y2
      dz = z - z2
   in potential b bs (e - m * m2 / sqrt (dx * dx + dy * dy + dz * dz))

-- Interaction of one pair: the updated pair.
pairStep :: Double -> Body -> Body -> (Body, Body)
pairStep dt (B x y z vx vy vz m) (B x2 y2 z2 vx2 vy2 vz2 m2) =
  let dx = x - x2
      dy = y - y2
      dz = z - z2
      d2 = dx * dx + dy * dy + dz * dz
      mag = dt / (d2 * sqrt d2)
      mj = m2 * mag
      mi = m * mag
   in ( B x y z (vx - dx * mj) (vy - dy * mj) (vz - dz * mj) m
      , B x2 y2 z2 (vx2 + dx * mi) (vy2 + dy * mi) (vz2 + dz * mi) m2 )

interactAll :: Double -> Body -> [Body] -> (Body, [Body])
interactAll _ b [] = (b, [])
interactAll dt b (c : cs) =
  case pairStep dt b c of
    (b1, c1) -> case interactAll dt b1 cs of
      (b2, cs1) -> (b2, c1 : cs1)

velocities :: Double -> [Body] -> [Body]
velocities _ [] = []
velocities dt (b : bs) = case interactAll dt b bs of
  (b1, bs1) -> b1 : velocities dt bs1

move :: Double -> [Body] -> [Body]
move _ [] = []
move dt (B x y z vx vy vz m : bs) = B (x + dt * vx) (y + dt * vy) (z + dt * vz) vx vy vz m : move dt bs

-- Forces the whole list (so no step is left as a thunk).
forceAll :: [Body] -> [Body]
forceAll bs = go bs `seq` bs
  where
    go [] = ()
    go (B _ _ _ _ _ _ _ : r) = go r

steps :: Int -> [Body] -> [Body]
steps 0 bs = bs
steps n bs = steps (n - 1) (forceAll (move 0.01 (velocities 0.01 bs)))

-- The value with 9 decimals (`%.9f`; aCHC has no printf).
fixed9 :: Double -> String
fixed9 x
  | x < 0 = '-' : fixed9 (negate x)
  | otherwise =
      let n = truncate (x * 1000000000 + 0.5) :: Int
          f = show (n `rem` 1000000000)
       in show (n `quot` 1000000000) ++ "." ++ replicate (9 - length f) '0' ++ f

main :: IO ()
main = do
  args <- getArgs
  let n = case args of { (a : _) -> read a; [] -> 1000 }
      bs = offset initial
  putStrLn (fixed9 (energy bs 0))
  putStrLn (fixed9 (energy (steps n bs) 0))
