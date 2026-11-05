module Config (Config (..), defaultConfig, describe) where

data Config = Config
  { name :: String
  , verbose :: Bool
  , level :: Int
  } deriving (Show)

defaultConfig :: Config
defaultConfig = Config { name = "default", verbose = False, level = 1 }

describe :: Config -> String
describe c = name c ++ (if verbose c then " (verbose)" else "") ++ " at " ++ show (level c)
