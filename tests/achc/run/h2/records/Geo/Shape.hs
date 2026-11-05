-- | Records with shared fields, exported with their fields.
module Geo.Shape
  ( Shape (..)
  , Point (Point, px)
  , mkPoint
  , py
  , area
  , origin
  ) where

data Point = Point { px :: Double, py :: Double }
  deriving (Show, Eq)

data Shape
  = Circle { center :: Point, radius :: Double }
  | Rect { corner :: Point, width, height :: Double }
  deriving (Show, Eq)

mkPoint :: Double -> Double -> Point
mkPoint x y = Point { px = x, py = y }

origin :: Point
origin = Point 0 0

area :: Shape -> Double
area Circle { radius = r } = 3 * r * r
area (Rect _ w h) = w * h
