-- opt-levels: 2
-- max-allocs: 15000
-- max-forced: 15000
-- Interactions of pairs of bodies (n-body, FEAT-0083): the shared factor
-- `mag` and both updated bodies are cheap floating-point arithmetic and
-- `sqrt` on evaluated fields, so they are computed right away instead of
-- becoming thunks (before: 129 thousand allocations and 78 thousand
-- forced thunks here). What is left are the lazy tails of the lists.
module Main where

data Body = B !Double !Double !Double !Double

pairStep :: Double -> Body -> Body -> (Body, Body)
pairStep dt (B x y v m) (B x2 y2 v2 m2) =
  let dx = x - x2
      dy = y - y2
      d2 = dx * dx + dy * dy
      mag = dt / (d2 * sqrt d2)
   in (B x y (v - dx * m2 * mag) m, B x2 y2 (v2 + dx * m * mag) m2)

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
move dt (B x y v m : bs) = B (x + dt * v) y v m : move dt bs

total :: [Body] -> Double -> Double
total [] acc = acc
total (B x _ v _ : bs) acc = total bs (acc + x + v)

steps :: Int -> [Body] -> [Body]
steps 0 bs = bs
steps n bs =
  let bs' = move 0.01 (velocities 0.01 bs)
   in total bs' 0 `seq` steps (n - 1) bs'

main :: IO ()
main = do
  let bodies = [B (fromIntegral i) (fromIntegral (i * i)) 0 (1 + fromIntegral i) | i <- [1 .. 6 :: Int]]
  print (truncate (total (steps 1000 bodies) 0 * 1000) :: Int)
