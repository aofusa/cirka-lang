-- | The program's command line (base's System.Environment).
module System.Environment (module System.Environment) where

import Cirka.Base
import Cirka.Prim

getArgs :: IO [String]
getArgs = primArgCount >>= \n -> mapM primGetArg (cirkaIntFromTo 0 (n - 1))

getProgName :: IO String
getProgName = primProgName
