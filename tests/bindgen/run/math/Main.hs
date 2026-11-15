-- The module Math is written by `achc bindgen math.h` (the C library's
-- math.h): functions declared pure, an output parameter, constants from
-- macros and from an anonymous enum.
import Foreign
import Math

main :: IO ()
main = do
  print (c_sqrt 2, c_pow 2 10, c_floor (-1.5))
  print (c_sin 0, c_cos 0)
  print (c_M_PI, c_M_E)
  print (c_FP_NAN, c_FP_NORMAL)
  x <- c_ldexp 1 10
  print x
  alloca $ \e -> do
    m <- c_frexp 8 e
    k <- peek e
    print (m, k)
