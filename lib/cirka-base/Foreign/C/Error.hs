-- | C errors (base's Foreign.C.Error, reduced; FEAT-0047): the `errno` of
-- the calling thread, read right after the C call that failed (08-ffi.md
-- §7.2), and the `throwErrno` family.
module Foreign.C.Error
  ( Errno(..), isValidErrno, getErrno, errnoToIOError
  , throwErrno, throwErrnoIf, throwErrnoIf_, throwErrnoIfMinus1, throwErrnoIfMinus1_
  , throwErrnoIfNull
  , eOK, ePERM, eNOENT, eSRCH, eINTR, eIO, eNXIO, e2BIG, eNOEXEC, eBADF, eCHILD
  , eAGAIN, eNOMEM, eACCES, eFAULT, eBUSY, eEXIST, eXDEV, eNODEV, eNOTDIR, eISDIR
  , eINVAL, eNFILE, eMFILE, eNOTTY, eFBIG, eNOSPC, eSPIPE, eROFS, eMLINK, ePIPE
  , eRANGE, eDEADLK, eNAMETOOLONG, eNOSYS, eNOTEMPTY, eWOULDBLOCK, eADDRINUSE
  , eCONNRESET, eTIMEDOUT, eCONNREFUSED
  ) where

import Cirka.Base (IOError(..))
import Cirka.Prim (primErrno, primStrerror)
import Foreign.C.Types
import Foreign.Ptr
import System.IO (Handle)

-- | A C error number.
newtype Errno = Errno CInt

instance Eq Errno where
  e1@(Errno a) == e2@(Errno b) = isValidErrno e1 && isValidErrno e2 && a == b

-- | Not the invalid number -1.
isValidErrno :: Errno -> Bool
isValidErrno (Errno e) = e /= -1

-- | The `errno` of the calling thread.
getErrno :: IO Errno
getErrno = fmap (Errno . fromIntegral) primErrno

-- | The error of a failed operation `loc` with `errno` `e`, shown as GHC
-- shows it: @loc: kind (description)@. The handle is not shown.
errnoToIOError :: String -> Errno -> Maybe Handle -> Maybe String -> IOError
errnoToIOError loc (Errno e) _ file =
  IOError (prefix ++ loc ++ ": " ++ kind n ++ " (" ++ primStrerror n ++ ")")
  where
    n = fromIntegral e
    prefix = maybe "" (++ ": ") file

-- | The kind of error GHC reports for an errno.
kind :: Int -> String
kind n
  | n `elem` [1, 13, 30] = "permission denied"
  | n `elem` [2, 3, 6, 19, 111] = "does not exist"
  | n == 17 = "already exists"
  | n `elem` [16, 26, 35] = "resource busy"
  | n `elem` [11, 12, 23, 24, 28, 31] = "resource exhausted"
  | n == 4 = "interrupted"
  | n `elem` [20, 21] = "inappropriate type"
  | n `elem` [29, 38] = "unsupported operation"
  | n `elem` [32, 104] = "resource vanished"
  | n == 110 = "timeout"
  | n == 5 = "hardware fault"
  | n `elem` [7, 9, 14, 22, 25, 27, 34, 36, 39] = "invalid argument"
  | otherwise = "failed"

-- | Fails with the current errno.
throwErrno :: String -> IO a
throwErrno loc = do
  e <- getErrno
  ioError (errnoToIOError loc e Nothing Nothing)

throwErrnoIf :: (a -> Bool) -> String -> IO a -> IO a
throwErrnoIf bad loc f = do
  r <- f
  if bad r then throwErrno loc else return r

throwErrnoIf_ :: (a -> Bool) -> String -> IO a -> IO ()
throwErrnoIf_ bad loc f = throwErrnoIf bad loc f >> return ()

throwErrnoIfMinus1 :: (Eq a, Num a) => String -> IO a -> IO a
throwErrnoIfMinus1 = throwErrnoIf (== -1)

throwErrnoIfMinus1_ :: (Eq a, Num a) => String -> IO a -> IO ()
throwErrnoIfMinus1_ = throwErrnoIf_ (== -1)

throwErrnoIfNull :: String -> IO (Ptr a) -> IO (Ptr a)
throwErrnoIfNull = throwErrnoIf (== nullPtr)

eOK, ePERM, eNOENT, eSRCH, eINTR, eIO, eNXIO, e2BIG, eNOEXEC, eBADF, eCHILD :: Errno
eOK = Errno 0
ePERM = Errno 1
eNOENT = Errno 2
eSRCH = Errno 3
eINTR = Errno 4
eIO = Errno 5
eNXIO = Errno 6
e2BIG = Errno 7
eNOEXEC = Errno 8
eBADF = Errno 9
eCHILD = Errno 10

eAGAIN, eNOMEM, eACCES, eFAULT, eBUSY, eEXIST, eXDEV, eNODEV, eNOTDIR, eISDIR :: Errno
eAGAIN = Errno 11
eNOMEM = Errno 12
eACCES = Errno 13
eFAULT = Errno 14
eBUSY = Errno 16
eEXIST = Errno 17
eXDEV = Errno 18
eNODEV = Errno 19
eNOTDIR = Errno 20
eISDIR = Errno 21

eINVAL, eNFILE, eMFILE, eNOTTY, eFBIG, eNOSPC, eSPIPE, eROFS, eMLINK, ePIPE :: Errno
eINVAL = Errno 22
eNFILE = Errno 23
eMFILE = Errno 24
eNOTTY = Errno 25
eFBIG = Errno 27
eNOSPC = Errno 28
eSPIPE = Errno 29
eROFS = Errno 30
eMLINK = Errno 31
ePIPE = Errno 32

eRANGE, eDEADLK, eNAMETOOLONG, eNOSYS, eNOTEMPTY, eWOULDBLOCK, eADDRINUSE :: Errno
eRANGE = Errno 34
eDEADLK = Errno 35
eNAMETOOLONG = Errno 36
eNOSYS = Errno 38
eNOTEMPTY = Errno 39
eWOULDBLOCK = Errno 11
eADDRINUSE = Errno 98

eCONNRESET, eTIMEDOUT, eCONNREFUSED :: Errno
eCONNRESET = Errno 104
eTIMEDOUT = Errno 110
eCONNREFUSED = Errno 111
