-- skip-backend: interp
-- Function pointers in the Haskell 2010 FFI (FEAT-0047, 08-ffi.md §7.2):
-- "wrapper" makes C function pointers of Haskell functions (closures),
-- "dynamic" calls function pointers, "&name" takes the address of a C
-- function, freeHaskellFunPtr frees a wrapper's pointer, and getErrno reads
-- the errno of a failed C call. C calls back into Haskell, so this runs
-- natively only.
import Data.IORef
import Data.List (sort)
import Foreign
import Foreign.C

type Compare = Ptr CLong -> Ptr CLong -> IO CInt

foreign import ccall "stdlib.h qsort"
  c_qsort :: Ptr CLong -> CSize -> CSize -> FunPtr Compare -> IO ()
foreign import ccall "wrapper" mkCompare :: Compare -> IO (FunPtr Compare)

type Step = CInt -> CDouble -> IO CDouble

foreign import ccall "wrapper" mkStep :: Step -> IO (FunPtr Step)
foreign import ccall "dynamic" callStep :: FunPtr Step -> Step

foreign import ccall "wrapper" mkPure :: (CLong -> CLong) -> IO (FunPtr (CLong -> CLong))
foreign import ccall "dynamic" callPure :: FunPtr (CLong -> CLong) -> CLong -> CLong
foreign import ccall "&labs" p_labs :: FunPtr (CLong -> CLong)

-- A polymorphic wrapper: the pointer's element type is erased.
foreign import ccall "wrapper" mkPeek :: (Ptr a -> IO ()) -> IO (FunPtr (Ptr a -> IO ()))
foreign import ccall "dynamic" callPeek :: FunPtr (Ptr a -> IO ()) -> Ptr a -> IO ()

foreign import ccall "&free" p_free :: FunPtr (Ptr () -> IO ())
foreign import ccall "unistd.h close" c_close :: CInt -> IO CInt

sortBy' :: Bool -> [CLong] -> IO [CLong]
sortBy' down xs = do
  calls <- newIORef (0 :: Int)
  cmp <- mkCompare $ \pa pb -> do
    modifyIORef calls (+ 1)
    a <- peek pa
    b <- peek pb
    let c = fromIntegral (fromEnum (compare a b)) - 1
    return (if down then negate c else c)
  ys <- withArrayLen xs $ \n p -> do
    c_qsort p (fromIntegral n) (fromIntegral (sizeOf (head xs))) cmp
    peekArray n p
  freeHaskellFunPtr cmp
  n <- readIORef calls
  print (n > 0)
  return ys

main :: IO ()
main = do
  let xs = [5, -1, 9, 3, 3, 0, 7, -4]
  up <- sortBy' False xs
  down <- sortBy' True xs
  print (up, down, up == sort xs)
  -- A closure over k, called through C's calling convention.
  let k = 3 :: CDouble
  step <- mkStep (\i x -> return (x * k + fromIntegral i))
  r <- callStep step 4 1.5
  freeHaskellFunPtr step
  print r
  twice <- mkPure (* 2)
  print (callPure twice 21, callPure p_labs (-42))
  freeHaskellFunPtr twice
  seen <- newIORef []
  pk <- mkPeek (\p -> peek (castPtr p) >>= \v -> modifyIORef seen (v :))
  with (11 :: CInt) $ \p -> callPeek pk (castPtr p :: Ptr ())
  with (22 :: CInt) $ \p -> callPeek pk (castPtr p)
  freeHaskellFunPtr pk
  readIORef seen >>= print . (reverse :: [CInt] -> [CInt])
  print (p_free /= nullFunPtr, castFunPtrToPtr p_labs /= nullPtr)
  rc <- c_close (-1)
  e <- getErrno
  print (rc, e == eBADF, e == eINTR)
