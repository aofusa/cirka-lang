-- | Re-exports a whole module and adds to it.
module Geo (module Geo.Shape, module Geo, scale) where

import Geo.Shape
import qualified Data.List as L

scale :: Double -> Shape -> Shape
scale k s@Circle {} = s { radius = radius s * k }
scale k s = s { width = width s * k, height = height s * k }

totalArea :: [Shape] -> Double
totalArea = L.foldl' (\a s -> a + area s) 0
