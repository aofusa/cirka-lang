module Main where

class Describe a where
  describe :: a -> String

class Default a where
  def :: a

instance Describe Bool where
  describe _ = "bool"

noInstance :: String
noInstance = describe 'c'

ambiguous :: String
ambiguous = describe def

main = return ()
