-- records with update syntax and Maybe/Either pipelines
import Data.Maybe (mapMaybe, fromMaybe, catMaybes)
import Data.Either (lefts, rights, partitionEithers)

data Person = Person { name :: String, age :: Int, email :: Maybe String }
  deriving (Show, Eq)

birthday :: Person -> Person
birthday p = p { age = age p + 1 }

validate :: Person -> Either String Person
validate p
  | null (name p) = Left "empty name"
  | age p < 0 = Left ("bad age " ++ show (age p))
  | otherwise = Right p

parseAge :: String -> Maybe Int
parseAge s | not (null s) && all (`elem` "0123456789") s = Just (read s)
           | otherwise = Nothing

main :: IO ()
main = do
  let a = Person { name = "Ann", age = 30, email = Nothing }
      b = a { name = "Bob", email = Just "bob@x.org" }
  print a
  print b
  print (birthday b)
  print (map validate [a, b { age = -4 }, a { name = "" }])
  print (mapMaybe parseAge ["12", "x", "7", ""])
  print (fromMaybe 0 (parseAge "zz"))
  print (parseAge "5" >>= \n -> if n > 3 then Just (n * 2) else Nothing)
  print (partitionEithers (map validate [a, b { age = -1 }]))
  print (lefts [Left 'a', Right (1 :: Int)], rights [Left 'a', Right (1 :: Int)])
  print (catMaybes [email a, email b])
  print (sequence [Just 1, Just (2 :: Int)], sequence [Just 1, Nothing :: Maybe Int])
  print (traverse parseAge ["1", "22"])
