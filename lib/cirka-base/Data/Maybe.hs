-- | The Maybe type and operations (base's Data.Maybe).
module Data.Maybe (Maybe(..), maybe, module Data.Maybe) where

import Cirka.Base

isJust :: Maybe a -> Bool
isJust (Just _) = True
isJust Nothing = False

isNothing :: Maybe a -> Bool
isNothing = not . isJust

fromJust :: Maybe a -> a
fromJust (Just x) = x
fromJust Nothing = errorWithoutStackTrace "Maybe.fromJust: Nothing"

fromMaybe :: a -> Maybe a -> a
fromMaybe d Nothing = d
fromMaybe _ (Just x) = x

maybeToList :: Maybe a -> [a]
maybeToList Nothing = []
maybeToList (Just x) = [x]

listToMaybe :: [a] -> Maybe a
listToMaybe [] = Nothing
listToMaybe (x:_) = Just x

catMaybes :: [Maybe a] -> [a]
catMaybes ms = [x | Just x <- ms]

mapMaybe :: (a -> Maybe b) -> [a] -> [b]
mapMaybe _ [] = []
mapMaybe f (x:xs) = case f x of
  Nothing -> mapMaybe f xs
  Just y -> y : mapMaybe f xs
