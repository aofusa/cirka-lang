-- bank account transactions with Either errors
data Tx = Deposit Int | Withdraw Int | Transfer String Int deriving Show

data BankError
  = Insufficient Int Int
  | InvalidAmount Int
  | UnknownAccount String
  deriving (Show, Eq)

type Account = (String, Int)

applyTx :: [String] -> Account -> Tx -> Either BankError Account
applyTx _ (n, b) (Deposit a)
  | a <= 0 = Left (InvalidAmount a)
  | otherwise = Right (n, b + a)
applyTx _ (n, b) (Withdraw a)
  | a <= 0 = Left (InvalidAmount a)
  | a > b = Left (Insufficient b a)
  | otherwise = Right (n, b - a)
applyTx known (n, b) (Transfer to a)
  | to `notElem` known = Left (UnknownAccount to)
  | a <= 0 = Left (InvalidAmount a)
  | a > b = Left (Insufficient b a)
  | otherwise = Right (n, b - a)

runAll :: [String] -> Account -> [Tx] -> Either BankError Account
runAll known = go
  where
    go acc [] = Right acc
    go acc (t : ts) = applyTx known acc t >>= \acc' -> go acc' ts

runLog :: [String] -> Account -> [Tx] -> [String]
runLog _ _ [] = []
runLog known acc (t : ts) = case applyTx known acc t of
  Left e -> ["FAIL " ++ show t ++ ": " ++ show e] ++ runLog known acc ts
  Right acc' -> ("OK " ++ show t ++ " -> " ++ show (snd acc')) : runLog known acc' ts

main :: IO ()
main = do
  let known = ["alice", "bob"]
      acc = ("alice", 100)
  print (runAll known acc [Deposit 50, Withdraw 30])
  print (runAll known acc [Withdraw 500])
  print (runAll known acc [Deposit (-5)])
  print (runAll known acc [Transfer "carol" 10])
  print (runAll known acc [Transfer "bob" 60, Withdraw 40])
  print (runAll known acc [Transfer "bob" 60, Withdraw 41])
  mapM_ putStrLn (runLog known acc [Deposit 20, Withdraw 200, Transfer "bob" 70, Transfer "zed" 1, Withdraw 0, Withdraw 50])
