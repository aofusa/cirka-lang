-- The module Shapes is written by `achc bindgen shapes.h`; shapes.c is
-- linked with the program.
import Foreign
import Foreign.C
import Shapes

label :: String -> [CChar]
label s = map castCharToCChar s ++ replicate (6 - length s) 0

-- The bit-field unit: `visible` is bit 0, `layer` bits 1..8.
bits :: Bool -> Word8 -> Word8
bits visible layer = fromIntegral (fromEnum visible) + 2 * layer

main :: IO ()
main = do
  print (c_SHAPES_VERSION, c_SHAPES_EPSILON)
  putStrLn c_SHAPES_NAME
  print [c_SHAPE_NONE, c_SHAPE_CIRCLE, c_SHAPE_RECT]
  print (sizeOf (undefined :: Shape), alignment (undefined :: Shape))
  let circle = Shape c_SHAPE_CIRCLE (Vec2 0 0) [2, 0] (bits True 3) (label "ring")
      rect = Shape c_SHAPE_RECT (Vec2 1 1) [3, 4] (bits True 5) (label "box")
      hidden = Shape c_SHAPE_RECT (Vec2 0 0) [1, 1] (bits False 7) (label "")
  -- Structs are written and read at clang's offsets.
  with rect $ \p -> do
    area <- c_shape_area p
    print area
    with (Vec2 0.5 (-2)) (c_shape_move p)
    r <- peek p
    print (shape_center r)
    print (r == rect { shape_center = Vec2 1.5 (-1) })
    putStrLn (map castCCharToChar (takeWhile (/= 0) (shape_label r)))
  withArray [circle, rect, hidden] $ \arr -> do
    -- A Haskell function as a C callback.
    count <- wrap_Visitor (\_ _ -> return 1)
    n <- c_shapes_visit arr 3 count nullPtr
    freeHaskellFunPtr count
    print n
    -- A C function pointer, called from Haskell and passed back to C.
    layerOf <- c_shapes_layer_visitor
    l <- dyn_Visitor layerOf arr nullPtr
    total <- c_shapes_visit arr 3 layerOf nullPtr
    print (l, total)
  print (c_lerp 1 3 0.25)
  -- A union is its bytes.
  with (Bits [1, 2, 3, 4]) $ \p -> do
    w <- c_bits_reverse p
    Bits bs <- peek p
    print (w, bs)
