-- Records and a program of several modules (FEAT-0115).
module Main (main) where

import Geo
import qualified Config as C
import Config (Config (..))
import Data.List hiding (insert)
import qualified Data.Maybe as M (fromMaybe)

insert :: Int -> [Int] -> [Int]
insert x xs = x : xs

main :: IO ()
main = do
  let p = mkPoint 1 2
      c = Circle { center = p, radius = 2 }
      r = Rect origin 3 4
  print p
  print (px p, py p, p { py = 5 })
  print [c, r]
  print (map area [c, r], totalArea [c, r])
  print (scale 2 c, scale 0.5 r)
  print (center c == p, corner r, width r)
  let cfg = C.defaultConfig { level = 3, verbose = True }
  putStrLn (C.describe cfg)
  putStrLn (C.describe C.defaultConfig { name = "other" })
  print cfg
  print (sort (insert 3 [2, 1]), M.fromMaybe 0 (Just (level cfg)))
  case cfg of
    Config { name = n, level = l } -> print (n, l)
