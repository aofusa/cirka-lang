-- BUG-0006: a data type whose strict field is a newtype of a function
-- returning the data type itself (from GHC's cgrun050).
module Main where

newtype F = F (Int -> Val)

data Val = VFn !F | VInt !Int

newtype Stream = Stream (Int, Maybe Stream)

apply :: Val -> Int -> Val
apply (VFn (F g)) x = g x
apply v _ = v

describe :: Val -> String
describe (VInt n) = "int " ++ show n
describe (VFn _) = "function"

total :: Stream -> Int
total (Stream (n, rest)) = n + maybe 0 total rest

main :: IO ()
main = do
  let v = VFn (F (\x -> VInt (x + 3)))
  putStrLn (describe v)
  putStrLn (describe (apply v 4))
  print (total (Stream (1, Just (Stream (2, Nothing)))))
