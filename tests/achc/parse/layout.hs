-- Layout (Report §10.3): nested implicit blocks, explicit braces,
-- parse-error(t) and DoAndIfThenElse.
module Layout where

f x = let y = x + 1
          z = y * 2
      in y + z

g x = case x of
  0 -> "zero"
  n | n < 0     -> "negative"
    | otherwise -> "positive"

h = let { a = 1; b = 2 } in a + b

k = [ (i, j) | i <- [1 .. 3], let j = i * i, odd j ]

main :: IO ()
main = do
  let xs = map f [1, 2, 3]
  if null xs
  then return ()
  else mapM_ print xs
  putStrLn (g 1) where
    unused = ()

tricky = (case 1 of 1 -> 2) + (let x = 3 in x)
