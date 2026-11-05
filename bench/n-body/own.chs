-- n-body (Benchmarks Game) with ownership: the 5 bodies live in one unique
-- unboxed array of doubles (7 per body: position, velocity, mass) that each
-- step updates in place by threading it linearly (`U.read` / `U.write`), as
-- rust/main.rs does with `&mut [Body]`. Same output as rust/main.rs.
-- (The same loop through `borrowMut` and the `Borrow` monad is ~130 times
-- slower, BUG-0011.)
module Main where

import Cirka.Ownership
import qualified Cirka.Array.Unboxed as U
import System.Environment (getArgs)

type Bodies = Unique (U.UArray Double)

solarMass, daysPerYear :: Double
solarMass = 4 * pi * pi
daysPerYear = 365.24

planet :: [Double] -> [Double]
planet [x, y, z, vx, vy, vz, m] = [x, y, z, vx * daysPerYear, vy * daysPerYear, vz * daysPerYear, m * solarMass]
planet _ = []

initial :: [Double]
initial =
  [0, 0, 0, 0, 0, 0, solarMass]
    ++ planet [ 4.84143144246472090, -1.16032004402742839, -0.103622044471123109
              , 0.00166007664274403694, 0.00769901118419740425, -0.0000690460016972063023
              , 0.000954791938424326609 ]
    ++ planet [ 8.34336671824457987, 4.12479856412430479, -0.403523417114321381
              , -0.00276742510726862411, 0.00499852801234917238, 0.0000230417297573763929
              , 0.000285885980666130812 ]
    ++ planet [ 12.8943695621391310, -15.1111514016986312, -0.223307578892655734
              , 0.00296460137564761618, 0.00237847173959480950, -0.0000296589568540237556
              , 0.0000436624404335156298 ]
    ++ planet [ 15.3796971148509165, -25.9193146099879641, 0.179258772950371181
              , 0.00268067772490389322, 0.00162824170038242295, -0.0000951592254519715870
              , 0.0000515138902046611451 ]

nb :: Int
nb = 5

-- ---- reading (shared array) ----

energy :: U.UArray Double -> Double
energy a = go 0 0
  where
    at i k = a U.! (i * 7 + k)
    go i !e
      | i >= nb = e
      | otherwise =
          let vx = at i 3
              vy = at i 4
              vz = at i 5
              m = at i 6
           in go (i + 1) (pot i (i + 1) (e + 0.5 * m * (vx * vx + vy * vy + vz * vz)))
    pot i j !e
      | j >= nb = e
      | otherwise =
          let dx = at i 0 - at j 0
              dy = at i 1 - at j 1
              dz = at i 2 - at j 2
           in pot i (j + 1) (e - at i 6 * at j 6 / sqrt (dx * dx + dy * dy + dz * dz))

offset :: U.UArray Double -> (Double, Double, Double)
offset a = go 0 0 0 0
  where
    go i !px !py !pz
      | i >= nb = (px, py, pz)
      | otherwise =
          let m = a U.! (i * 7 + 6)
           in go (i + 1) (px + a U.! (i * 7 + 3) * m) (py + a U.! (i * 7 + 4) * m) (pz + a U.! (i * 7 + 5) * m)

-- ---- updating in place (the array is threaded linearly) ----

get :: Bodies %1 -> Int -> (Bodies, Ur Double)
get = U.read

add :: Bodies %1 -> Int -> Double -> Bodies
add u k d = case U.read u k of
  (u', Ur v) -> U.write u' k (v + d)

advance :: Double -> Bodies %1 -> Bodies
advance dt u = move (pairs u 0 1) 0
  where
    pairs u i j
      | i >= nb = u
      | j >= nb = pairs u (i + 1) (i + 2)
      | otherwise =
          case get u (i * 7) of { (u, Ur x) ->
          case get u (i * 7 + 1) of { (u, Ur y) ->
          case get u (i * 7 + 2) of { (u, Ur z) ->
          case get u (i * 7 + 6) of { (u, Ur mi0) ->
          case get u (j * 7) of { (u, Ur x2) ->
          case get u (j * 7 + 1) of { (u, Ur y2) ->
          case get u (j * 7 + 2) of { (u, Ur z2) ->
          case get u (j * 7 + 6) of { (u, Ur mj0) ->
            let dx = x - x2
                dy = y - y2
                dz = z - z2
                d2 = dx * dx + dy * dy + dz * dz
                mag = dt / (d2 * sqrt d2)
                mj = mj0 * mag
                mi = mi0 * mag
                u1 = add (add (add u (i * 7 + 3) (negate (dx * mj))) (i * 7 + 4) (negate (dy * mj))) (i * 7 + 5) (negate (dz * mj))
                u2 = add (add (add u1 (j * 7 + 3) (dx * mi)) (j * 7 + 4) (dy * mi)) (j * 7 + 5) (dz * mi)
             in pairs u2 i (j + 1) }}}}}}}}
    move u i
      | i >= nb = u
      | otherwise =
          case get u (i * 7 + 3) of { (u, Ur vx) ->
          case get u (i * 7 + 4) of { (u, Ur vy) ->
          case get u (i * 7 + 5) of { (u, Ur vz) ->
            move (add (add (add u (i * 7) (dt * vx)) (i * 7 + 1) (dt * vy)) (i * 7 + 2) (dt * vz)) (i + 1) }}}

steps :: Int -> Bodies %1 -> Bodies
steps 0 b = b
steps n b = steps (n - 1) (advance 0.01 b)

-- The value with 9 decimals (`%.9f`).
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
      a0 = U.fromList initial
      (px, py, pz) = offset a0
      a1 = U.generate (nb * 7) (\k -> case k of
             3 -> negate px / solarMass
             4 -> negate py / solarMass
             5 -> negate pz / solarMass
             _ -> a0 U.! k)
  putStrLn (fixed9 (energy a1))
  putStrLn (fixed9 (energy (freeze (steps n (U.thaw a1)))))
