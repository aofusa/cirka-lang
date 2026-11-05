-- | Standard input and output (base's System.IO): the three standard
-- handles and whole-file operations. Opening other handles is not
-- supported yet.
module System.IO
  ( IO, FilePath, putChar, putStr, putStrLn, print, getChar, getLine, getContents
  , interact, readIO, readLn, readFile, writeFile, appendFile, module System.IO
  ) where

import Cirka.Base
import Cirka.Prim

data Handle = Stdin | Stdout | Stderr
  deriving (Eq)

instance Show Handle where
  show Stdin = "<stdin>"
  show Stdout = "<stdout>"
  show Stderr = "<stderr>"

stdin, stdout, stderr :: Handle
stdin = Stdin
stdout = Stdout
stderr = Stderr

data BufferMode = NoBuffering | LineBuffering | BlockBuffering (Maybe Int)
  deriving (Eq, Ord, Show, Read)

-- | Output is flushed at exit, before reading input and by `hFlush`;
-- buffering modes are accepted and ignored.
hSetBuffering :: Handle -> BufferMode -> IO ()
hSetBuffering _ _ = return ()

hFlush :: Handle -> IO ()
hFlush _ = primFlush

-- | Whether standard input is at its end.
isEOF :: IO Bool
isEOF = primPeekByte >>= \b -> return (b < 0)

hPutStr :: Handle -> String -> IO ()
hPutStr Stderr s = cirkaPutErr s
hPutStr _ s = putStr s

hPutStrLn :: Handle -> String -> IO ()
hPutStrLn h s = hPutStr h (s ++ "\n")

hPrint :: Show a => Handle -> a -> IO ()
hPrint h x = hPutStrLn h (show x)

hPutChar :: Handle -> Char -> IO ()
hPutChar h c = hPutStr h [c]

hGetLine :: Handle -> IO String
hGetLine _ = getLine

hGetContents :: Handle -> IO String
hGetContents _ = getContents

hGetChar :: Handle -> IO Char
hGetChar _ = getChar
