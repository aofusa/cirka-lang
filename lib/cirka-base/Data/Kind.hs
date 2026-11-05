-- | Kinds (base's Data.Kind). Only for import lists: kind signatures
-- (KindSignatures) are read by the parser, where `Type` and `*` both mean
-- the kind of value types. `LType` is aCHC's kind of linear-only types
-- (03-type-system.md §1).
module Data.Kind (Type, LType) where

data Type

data LType
