-- | C strings (base's Foreign.C.String). Strings are UTF-8.
module Foreign.C.String
  ( CString, CStringLen, newCString, newCStringLen, peekCString, peekCStringLen
  , withCString, withCStringLen, newCAString, peekCAString, withCAString
  , castCharToCChar, castCCharToChar, castCharToCUChar, castCUCharToChar
  ) where

import Cirka.Foreign.Marshal
