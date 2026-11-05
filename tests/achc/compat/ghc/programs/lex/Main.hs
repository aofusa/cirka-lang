-- Imported from GHC 9.4.7 testsuite/tests/programs/lex/Main.hs (BSD-3-Clause; see LICENSE in this directory).
module Main where

main = interact ( \ s -> shows (lex' s) "\n")
     where lex' "" = []
           lex' s = tok : lex' s' where -- [(tok,s')] = lex s
                                        (tok,s') = case lex s of
                                                    [r]   -> r
                                                    []    -> error ("Empty: " ++ s)
                                                    other -> error ("Multi: " ++ s)
