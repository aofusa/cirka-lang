-- Imported from GHC 9.4.7 testsuite/tests/numeric/should_run/T12136.hs (BSD-3-Clause; see LICENSE in this directory).
{-# LANGUAGE CPP #-}

#include "MachDeps.h"

module Main where

import Data.Bits

#if WORD_SIZE_IN_BITS != 64 && WORD_SIZE_IN_BITS != 32
# error unsupported WORD_SIZE_IN_BITS config
#endif

-- a negative integer the size of GMP_LIMB_BITS*2
negativeBigInteger :: Integer
negativeBigInteger = 1 - (1 `shiftL` (64 * 2))

main = do
    -- right shift by GMP_LIMB_BITS
    print $ negativeBigInteger `shiftR` 64
