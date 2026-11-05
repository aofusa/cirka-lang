{-# LANGUAGE LinearTypes #-}
module Linear where

f :: a %1 -> b %0 -> a
f x _ = x

g :: (a %m -> b) -> a %m -> b %Many -> (b, b)
g h x y = (h x, y)

k :: a ⊸ a
k x = x
