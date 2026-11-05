-- stack-based VM interpreting a list of instructions
data Instr
  = Push Int
  | Pop
  | Add
  | Sub
  | Mul
  | Dup
  | Swap
  | Jz Int
  | Jmp Int
  | Print
  deriving (Show, Eq)

type VM = (Int, [Int], [String])

step :: [Instr] -> VM -> Either String VM
step prog (pc, st, out) = case (prog !! pc, st) of
  (Push n, _) -> Right (pc + 1, n : st, out)
  (Pop, _ : r) -> Right (pc + 1, r, out)
  (Add, a : b : r) -> Right (pc + 1, b + a : r, out)
  (Sub, a : b : r) -> Right (pc + 1, b - a : r, out)
  (Mul, a : b : r) -> Right (pc + 1, b * a : r, out)
  (Dup, a : r) -> Right (pc + 1, a : a : r, out)
  (Swap, a : b : r) -> Right (pc + 1, b : a : r, out)
  (Jz t, a : r) -> Right (if a == 0 then t else pc + 1, r, out)
  (Jmp t, _) -> Right (t, st, out)
  (Print, a : r) -> Right (pc + 1, r, show a : out)
  (i, _) -> Left ("stack underflow at " ++ show pc ++ ": " ++ show i)

run :: [Instr] -> Int -> VM -> Either String [String]
run prog fuel vm@(pc, _, out)
  | pc >= length prog = Right (reverse out)
  | fuel == 0 = Left "out of fuel"
  | otherwise = step prog vm >>= run prog (fuel - 1)

-- countdown from 5, printing each value
countdown :: [Instr]
countdown =
  [ Push 5
  , Dup, Print
  , Push 1, Sub
  , Dup, Jz 8
  , Jmp 1
  , Pop
  ]

main :: IO ()
main = do
  let go name p = putStrLn (name ++ ": " ++ either ("error: " ++) unwords (run p 1000 (0, [], [])))
  go "arith" [Push 2, Push 3, Add, Push 4, Mul, Print]
  go "swap" [Push 10, Push 3, Swap, Sub, Print]
  go "countdown" countdown
  go "underflow" [Push 1, Add]
  go "loop" [Jmp 0]
  go "empty" []
  go "dup" [Push 7, Dup, Mul, Print]
