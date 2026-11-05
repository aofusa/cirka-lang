-- | Ending the program (base's System.Exit). Output is flushed first.
module System.Exit (module System.Exit) where

import Cirka.Base
import Cirka.Prim

data ExitCode = ExitSuccess | ExitFailure Int
  deriving (Eq, Ord, Read, Show)

exitWith :: ExitCode -> IO a
exitWith ExitSuccess = primExit 0
exitWith (ExitFailure 0) =
  ioError (IOError "Control.Exception.Base.exitWith: invalid argument (ExitFailure 0)")
exitWith (ExitFailure n) = primExit n

exitFailure :: IO a
exitFailure = exitWith (ExitFailure 1)

exitSuccess :: IO a
exitSuccess = exitWith ExitSuccess

die :: String -> IO a
die err = cirkaPutErr (err ++ "\n") >> exitFailure
