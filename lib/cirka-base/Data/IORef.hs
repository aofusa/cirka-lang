-- | Mutable references in the IO monad (base's Data.IORef). A reference
-- whose value refers back to it forms a cycle that is never freed
-- (docs/design/04-memory-model.md §5).
module Data.IORef (IORef, module Data.IORef) where

import Cirka.Base
import Cirka.Prim

newIORef :: a -> IO (IORef a)
newIORef = primNewIORef

readIORef :: IORef a -> IO a
readIORef = primReadIORef

writeIORef :: IORef a -> a -> IO ()
writeIORef = primWriteIORef

modifyIORef :: IORef a -> (a -> a) -> IO ()
modifyIORef r f = readIORef r >>= \x -> writeIORef r (f x)

modifyIORef' :: IORef a -> (a -> a) -> IO ()
modifyIORef' r f = readIORef r >>= \x -> let x' = f x in x' `seq` writeIORef r x'

atomicModifyIORef :: IORef a -> (a -> (a, b)) -> IO b
atomicModifyIORef r f = readIORef r >>= \x -> let (x', b) = f x in writeIORef r x' >> return b

atomicModifyIORef' :: IORef a -> (a -> (a, b)) -> IO b
atomicModifyIORef' r f = readIORef r >>= \x -> case f x of
  (x', b) -> x' `seq` b `seq` (writeIORef r x' >> return b)

atomicWriteIORef :: IORef a -> a -> IO ()
atomicWriteIORef = writeIORef
