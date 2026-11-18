-- The module Counter is written by `achc bindgen --extern counter.h`: an
-- extern block with the opaque type `Counter` and the functions named as
-- in C (FEAT-0130). GHC does not run it (CirkaExtern).
import qualified Counter as C

main :: IO ()
main = do
  c <- C.counter_new 5
  a <- C.counter_next c
  b <- C.counter_next c
  print (a, b)
  C.counter_free c
