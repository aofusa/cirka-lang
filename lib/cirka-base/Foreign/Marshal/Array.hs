-- | Arrays in raw memory (base's Foreign.Marshal.Array).
module Foreign.Marshal.Array
  ( mallocArray, mallocArray0, allocaArray, allocaArray0, reallocArray, callocArray
  , peekArray, peekArray0, pokeArray, pokeArray0, newArray, newArray0
  , withArray, withArray0, withArrayLen, withArrayLen0, copyArray, moveArray
  , lengthArray0, advancePtr
  ) where

import Cirka.Foreign.Marshal
