-- | Pointers (base's Foreign.Ptr).
module Foreign.Ptr
  ( Ptr, FunPtr, nullPtr, castPtr, plusPtr, minusPtr, alignPtr
  , nullFunPtr, castFunPtr, castFunPtrToPtr, castPtrToFunPtr
  , IntPtr, WordPtr, ptrToIntPtr, intPtrToPtr, ptrToWordPtr, wordPtrToPtr
  ) where

import Cirka.Base
import Cirka.Foreign
import Cirka.Foreign.Types

type IntPtr = Int

type WordPtr = Word

ptrToIntPtr :: Ptr a -> IntPtr
ptrToIntPtr p = cirkaPtrAddr p

intPtrToPtr :: IntPtr -> Ptr a
intPtrToPtr = CirkaPtr

ptrToWordPtr :: Ptr a -> WordPtr
ptrToWordPtr p = CirkaWord (cirkaPtrAddr p)

wordPtrToPtr :: WordPtr -> Ptr a
wordPtrToPtr (CirkaWord a) = CirkaPtr a
