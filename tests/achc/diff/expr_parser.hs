-- tokenizer and recursive-descent parser/evaluator for arithmetic expressions
import Data.Char (isDigit, isSpace)

data Token = TNum Int | TPlus | TMinus | TStar | TSlash | TLParen | TRParen
  deriving (Show, Eq)

data Expr = Num Int | Bin Char Expr Expr | Neg Expr deriving Show

tokenize :: String -> Either String [Token]
tokenize [] = Right []
tokenize (c : cs)
  | isSpace c = tokenize cs
  | isDigit c = let (ds, rest) = span isDigit (c : cs)
                in (TNum (read ds) :) <$> tokenize rest
  | c == '+' = (TPlus :) <$> tokenize cs
  | c == '-' = (TMinus :) <$> tokenize cs
  | c == '*' = (TStar :) <$> tokenize cs
  | c == '/' = (TSlash :) <$> tokenize cs
  | c == '(' = (TLParen :) <$> tokenize cs
  | c == ')' = (TRParen :) <$> tokenize cs
  | otherwise = Left ("unexpected character " ++ show c)

-- expr := term (('+'|'-') term)*
parseExpr :: [Token] -> Either String (Expr, [Token])
parseExpr ts = do
  (t, rest) <- parseTerm ts
  loop t rest
  where
    loop l (TPlus : r) = do { (t, r') <- parseTerm r; loop (Bin '+' l t) r' }
    loop l (TMinus : r) = do { (t, r') <- parseTerm r; loop (Bin '-' l t) r' }
    loop l r = Right (l, r)

parseTerm :: [Token] -> Either String (Expr, [Token])
parseTerm ts = do
  (f, rest) <- parseFactor ts
  loop f rest
  where
    loop l (TStar : r) = do { (t, r') <- parseFactor r; loop (Bin '*' l t) r' }
    loop l (TSlash : r) = do { (t, r') <- parseFactor r; loop (Bin '/' l t) r' }
    loop l r = Right (l, r)

parseFactor :: [Token] -> Either String (Expr, [Token])
parseFactor (TNum n : r) = Right (Num n, r)
parseFactor (TMinus : r) = do { (f, r') <- parseFactor r; Right (Neg f, r') }
parseFactor (TLParen : r) = do
  (e, r') <- parseExpr r
  case r' of
    TRParen : r'' -> Right (e, r'')
    _ -> Left "expected )"
parseFactor ts = Left ("unexpected " ++ show (take 1 ts))

eval :: Expr -> Either String Int
eval (Num n) = Right n
eval (Neg e) = negate <$> eval e
eval (Bin op a b) = do
  x <- eval a
  y <- eval b
  case op of
    '+' -> Right (x + y)
    '-' -> Right (x - y)
    '*' -> Right (x * y)
    _ -> if y == 0 then Left "division by zero" else Right (x `div` y)

calc :: String -> Either String Int
calc s = do
  ts <- tokenize s
  (e, rest) <- parseExpr ts
  if null rest then eval e else Left ("trailing tokens: " ++ show rest)

main :: IO ()
main = mapM_ (\s -> putStrLn (s ++ " => " ++ either ("error: " ++) show (calc s)))
  [ "1 + 2 * 3"
  , "(1 + 2) * 3"
  , "10 / 3 - 1"
  , "-(4 + 5) * 2"
  , "2 * (3 + 4) * 5"
  , "1 / (2 - 2)"
  , "3 + $"
  , "(1 + 2"
  , "1 2"
  , "100 - 1 - 1 - 1"
  ]
