-- The base modules of FEAT-0111 (Data.List, Data.Char, Data.Maybe, ...).
import Data.List
import Data.Char
import Data.Maybe
import Data.Either
import Data.Function
import Data.Ord
import Data.Tuple
import qualified Data.Foldable as F
import Data.Traversable (for)
import Data.Functor
import Control.Monad
import Control.Applicative
import Text.Read (readMaybe)
import System.IO

main :: IO ()
main = do
  hSetBuffering stdout NoBuffering
  hFlush stdout
  print (sort [3, 1, 2], sortBy (comparing Down) [3, 1, 2], sortOn negate [1, 3, 2])
  print (nub [1, 1, 2, 3, 2], group "aabccc", intercalate ", " ["a", "b"], transpose ["abc", "de"])
  print (permutations [1, 2, 3], subsequences "abc")
  print (isPrefixOf "ab" "abc", isInfixOf "bc" "abcd", stripPrefix "foo" "foobar", partition even [1 .. 10])
  print (foldl' (+) 0 [1 .. 100], unfoldr (\n -> if n > 5 then Nothing else Just (n, n + 1)) 1)
  print (map toUpper "hello", digitToInt 'f', isAlpha 'x', ord 'A', chr 98)
  print (mapMaybe (\x -> if x > 1 then Just (x * 2) else Nothing) [1, 2, 3], catMaybes [Just 1, Nothing], fromMaybe 0 Nothing)
  print (partitionEithers [Left 1, Right 'a', Left 2], lefts [Left 1, Right 2])
  print (((+) `on` length) [1, 2] [3], [1, 2, 3] & map (* 2), swap (1, 'x'))
  print (F.toList (Just 3), F.foldl' (-) 10 [1, 2])
  r <- for [1, 2, 3] (\x -> return (x * 10))
  print r
  print (Just 1 $> 2, void (Just 3))
  forM_ [1, 2] print
  xs <- foldM (\acc x -> do { print x; return (acc + x) }) 0 [5, 6]
  print xs
  print (replicateM 2 "ab", join [[1], [2, 3]], guard True :: Maybe (), filterM (const [True, False]) [1, 2])
  print (Just 1 <|> Nothing, Nothing <|> Just 2, liftA2 (+) (Just 1) (Just 2), (readMaybe "12" :: Maybe Int), (readMaybe "x" :: Maybe Int))
  when True (putStrLn "when")
  unless True (putStrLn "unless")
  hPutStrLn stderr "to stderr"
  print (zipWith3 (,,) [1] "a" [True], lookup 2 (zip [1 ..] "abc"), elemIndex 3 [1, 2, 3], findIndices even [1, 2, 4])
  print (insert 3 [1, 2, 4, 5], delete 3 [1, 3, 2, 3], [1, 2, 3, 4] \\ [2, 3], union [1, 2] [2, 3], intersect [1, 2, 3] [2, 3, 4])
  print (inits [1, 2], tails [1, 2], mapAccumL (\a x -> (a + x, a * x)) 0 [1, 2, 3], maximumBy (comparing snd) [(1, 'a'), (2, 'c'), (3, 'b')])
