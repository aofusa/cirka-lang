# FEAT-0231: 等式・証明と証明付き標準型

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P0
- 依存: FEAT-0230
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

命題等価 `=`・`Refl`・`rewrite`・`with`、`Nat`/`Fin`/`Vect`/サイズ付き配列、消去の保証。

## 作業内容

- [x] 等式型と rewrite
- [x] with 抽象
- [x] `core` の Nat / Fin / Vect / Dec / So
- [x] 証明が消去されることの確認

## 受け入れ条件

- [x] `append` の長さ・`reverse` の性質などの証明例が通る
- [x] 証明付きコードと証明なしコードの生成コードが同一
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 仕様は [spec-v0.0.4.md](../../design/cirkalang/spec-v0.0.4.md) §2、ADR-078。
- `x = y` は Prelude の `Equal x y`。`rewrite` は `primRewrite` (J 相当) に展開し、ゴールは case で止まらない範囲で 1 段ずつ展開して探す。
- 証明例: `tests/cirkalang/run/v004/proofs.cirka` (`appendNilRight`・`appendAssoc`・`lengthAppend`・`revAppend`・`revRev`・`with`・`Dec`・`Vect` / `Fin`)。
- 消去: `proof_erased.cirka` と `proof_none.cirka` の `-O2` の MIR が同一であることを `cirkalang_proofs_are_erased` で確かめる。
  そのため Core 最適化で、構成子の分かった値の `cast` を既知の構成子として扱い、遅延のままにする。
- 「サイズ付き配列」は `Vect` で代える (配列の長さを型に持つ型は v0.0.5 以降の検討)。
- 制限: 型の中のトレイトのメソッド・多相リテラルは簡約されないことがある。

