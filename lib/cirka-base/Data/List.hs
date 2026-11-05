-- | Operations on lists (base's Data.List). `sortBy` and `permutations`
-- are ported from GHC's base (BSD-3-Clause, see ../LICENSE-GHC-BASE) so
-- that results (stability, order of permutations) are the same.
module Data.List
  ( module Data.List
  , map, (++), filter, head, last, tail, init, (!!), null, length, reverse
  , and, or, any, all, concat, concatMap, sum, product, maximum, minimum, elem, notElem
  , foldl, foldr, foldl1, foldr1
  , scanl, scanl1, scanr, scanr1, iterate, repeat, replicate, cycle
  , take, drop, takeWhile, dropWhile, span, break, splitAt, lookup
  , zip, zip3, zipWith, zipWith3, unzip, unzip3, lines, words, unlines, unwords
  , foldl', find, maximumBy, minimumBy
  ) where

import Cirka.Base

-- | Strict left fold.

foldl1' :: (a -> a -> a) -> [a] -> a
foldl1' f (x:xs) = foldl' f x xs
foldl1' _ [] = errorWithoutStackTrace "Prelude.foldl1': empty list"

uncons :: [a] -> Maybe (a, [a])
uncons [] = Nothing
uncons (x:xs) = Just (x, xs)

unsnoc :: [a] -> Maybe ([a], a)
unsnoc [] = Nothing
unsnoc xs = Just (init xs, last xs)

singleton :: a -> [a]
singleton x = [x]

intersperse :: a -> [a] -> [a]
intersperse _ [] = []
intersperse sep (x:xs) = x : go xs
  where go [] = []
        go (y:ys) = sep : y : go ys

intercalate :: [a] -> [[a]] -> [a]
intercalate xs xss = concat (intersperse xs xss)

transpose :: [[a]] -> [[a]]
transpose [] = []
transpose ([] : xss) = transpose xss
transpose ((x:xs) : xss) = (x : [h | (h:_) <- xss]) : transpose (xs : [t | (_:t) <- xss])

subsequences :: [a] -> [[a]]
subsequences xs = [] : nonEmptySubsequences xs

nonEmptySubsequences :: [a] -> [[a]]
nonEmptySubsequences [] = []
nonEmptySubsequences (x:xs) = [x] : foldr f [] (nonEmptySubsequences xs)
  where f ys r = ys : (x : ys) : r

-- | GHC's permutations (same order of results).
permutations :: [a] -> [[a]]
permutations xs0 = xs0 : perms xs0 []
  where
    perms [] _ = []
    perms (t:ts) is' = foldr interleave (perms ts (t : is')) (permutations is')
      where interleave xs r = let (_, zs) = interleave' id xs r in zs
            interleave' _ [] r = (ts, r)
            interleave' f (y:ys) r = let (us, zs) = interleave' (f . (y :)) ys r
                                     in (y : us, f (t : y : us) : zs)

iterate' :: (a -> a) -> a -> [a]
iterate' f x = x `seq` (x : iterate' f (f x))

scanl' :: (b -> a -> b) -> b -> [a] -> [b]
scanl' f q ls = q `seq` (q : (case ls of
  [] -> []
  x:xs -> scanl' f (f q x) xs))

mapAccumL :: (s -> a -> (s, b)) -> s -> [a] -> (s, [b])
mapAccumL _ s [] = (s, [])
mapAccumL f s (x:xs) = (s'', y : ys)
  where (s', y) = f s x
        (s'', ys) = mapAccumL f s' xs

mapAccumR :: (s -> a -> (s, b)) -> s -> [a] -> (s, [b])
mapAccumR _ s [] = (s, [])
mapAccumR f s (x:xs) = (s'', y : ys)
  where (s'', y) = f s' x
        (s', ys) = mapAccumR f s xs

unfoldr :: (b -> Maybe (a, b)) -> b -> [a]
unfoldr f b = case f b of
  Just (a, b') -> a : unfoldr f b'
  Nothing -> []

dropWhileEnd :: (a -> Bool) -> [a] -> [a]
dropWhileEnd p = foldr (\x xs -> if p x && null xs then [] else x : xs) []

stripPrefix :: Eq a => [a] -> [a] -> Maybe [a]
stripPrefix [] ys = Just ys
stripPrefix (x:xs) (y:ys) | x == y = stripPrefix xs ys
stripPrefix _ _ = Nothing

group :: Eq a => [a] -> [[a]]
group = groupBy (==)

groupBy :: (a -> a -> Bool) -> [a] -> [[a]]
groupBy _ [] = []
groupBy eq (x:xs) = (x : ys) : groupBy eq zs
  where (ys, zs) = span (eq x) xs

inits :: [a] -> [[a]]
inits xs = [] : case xs of
  [] -> []
  (y:ys) -> map (y :) (inits ys)

tails :: [a] -> [[a]]
tails xs = xs : case xs of
  [] -> []
  (_:ys) -> tails ys

isPrefixOf :: Eq a => [a] -> [a] -> Bool
isPrefixOf [] _ = True
isPrefixOf _ [] = False
isPrefixOf (x:xs) (y:ys) = x == y && isPrefixOf xs ys

isSuffixOf :: Eq a => [a] -> [a] -> Bool
isSuffixOf xs ys = reverse xs `isPrefixOf` reverse ys

isInfixOf :: Eq a => [a] -> [a] -> Bool
isInfixOf needle haystack = any (isPrefixOf needle) (tails haystack)

isSubsequenceOf :: Eq a => [a] -> [a] -> Bool
isSubsequenceOf [] _ = True
isSubsequenceOf _ [] = False
isSubsequenceOf a@(x:a') (y:b)
  | x == y = isSubsequenceOf a' b
  | otherwise = isSubsequenceOf a b

partition :: (a -> Bool) -> [a] -> ([a], [a])
partition p xs = foldr select ([], []) xs
  where select x ~(ts, fs) | p x = (x : ts, fs)
                           | otherwise = (ts, x : fs)

elemIndex :: Eq a => a -> [a] -> Maybe Int
elemIndex x = findIndex (x ==)

elemIndices :: Eq a => a -> [a] -> [Int]
elemIndices x = findIndices (x ==)

findIndex :: (a -> Bool) -> [a] -> Maybe Int
findIndex p xs = case findIndices p xs of
  [] -> Nothing
  (i:_) -> Just i

findIndices :: (a -> Bool) -> [a] -> [Int]
findIndices p xs = [i | (x, i) <- zip xs [0 ..], p x]

zip4 :: [a] -> [b] -> [c] -> [d] -> [(a, b, c, d)]
zip4 (a:as) (b:bs) (c:cs) (d:ds) = (a, b, c, d) : zip4 as bs cs ds
zip4 _ _ _ _ = []

zipWith4 :: (a -> b -> c -> d -> e) -> [a] -> [b] -> [c] -> [d] -> [e]
zipWith4 z (a:as) (b:bs) (c:cs) (d:ds) = z a b c d : zipWith4 z as bs cs ds
zipWith4 _ _ _ _ _ = []

nub :: Eq a => [a] -> [a]
nub = nubBy (==)

nubBy :: (a -> a -> Bool) -> [a] -> [a]
nubBy eq l = go l []
  where go [] _ = []
        go (y:ys) xs
          | any (\x -> eq x y) xs = go ys xs
          | otherwise = y : go ys (y : xs)

delete :: Eq a => a -> [a] -> [a]
delete = deleteBy (==)

deleteBy :: (a -> a -> Bool) -> a -> [a] -> [a]
deleteBy _ _ [] = []
deleteBy eq x (y:ys) = if eq x y then ys else y : deleteBy eq x ys

(\\) :: Eq a => [a] -> [a] -> [a]
(\\) = foldl (flip delete)

infix 5 \\

union :: Eq a => [a] -> [a] -> [a]
union = unionBy (==)

unionBy :: (a -> a -> Bool) -> [a] -> [a] -> [a]
unionBy eq xs ys = xs ++ foldl (flip (deleteBy eq)) (nubBy eq ys) xs

intersect :: Eq a => [a] -> [a] -> [a]
intersect = intersectBy (==)

intersectBy :: (a -> a -> Bool) -> [a] -> [a] -> [a]
intersectBy _ [] _ = []
intersectBy _ _ [] = []
intersectBy eq xs ys = [x | x <- xs, any (eq x) ys]

insert :: Ord a => a -> [a] -> [a]
insert = insertBy compare

insertBy :: (a -> a -> Ordering) -> a -> [a] -> [a]
insertBy _ x [] = [x]
insertBy cmp x ys@(y:ys') = case cmp x y of
  GT -> y : insertBy cmp x ys'
  _ -> x : ys

genericLength :: Num i => [a] -> i
genericLength xs = fromIntegral (length xs)

genericTake :: Integral i => i -> [a] -> [a]
genericTake n xs = take (fromIntegral n) xs

genericDrop :: Integral i => i -> [a] -> [a]
genericDrop n xs = drop (fromIntegral n) xs

genericSplitAt :: Integral i => i -> [a] -> ([a], [a])
genericSplitAt n xs = splitAt (fromIntegral n) xs

genericIndex :: Integral i => [a] -> i -> a
genericIndex xs n = xs !! fromIntegral n

genericReplicate :: Integral i => i -> a -> [a]
genericReplicate n x = replicate (fromIntegral n) x

sort :: Ord a => [a] -> [a]
sort = sortBy compare

sortOn :: Ord b => (a -> b) -> [a] -> [a]
sortOn f = map snd . sortBy (\a b -> compare (fst a) (fst b)) . map (\x -> let y = f x in y `seq` (y, x))

-- | GHC's stable merge sort on ascending and descending runs.
sortBy :: (a -> a -> Ordering) -> [a] -> [a]
sortBy cmp = mergeAll . sequences
  where
    sequences (a:b:xs)
      | a `cmp` b == GT = descending b [a] xs
      | otherwise = ascending b (a :) xs
    sequences xs = [xs]
    descending a as (b:bs)
      | a `cmp` b == GT = descending b (a : as) bs
    descending a as bs = (a : as) : sequences bs
    ascending a as (b:bs)
      | a `cmp` b /= GT = ascending b (\ys -> as (a : ys)) bs
    ascending a as bs = let x = as [a] in x `seq` (x : sequences bs)
    mergeAll [x] = x
    mergeAll xs = mergeAll (mergePairs xs)
    mergePairs (a:b:xs) = let x = merge a b in x `seq` (x : mergePairs xs)
    mergePairs xs = xs
    merge as@(a:as') bs@(b:bs')
      | a `cmp` b == GT = b : merge as bs'
      | otherwise = a : merge as' bs
    merge [] bs = bs
    merge as [] = as
