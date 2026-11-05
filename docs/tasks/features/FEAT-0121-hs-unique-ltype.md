# FEAT-0121: 種 LType と Unique 型

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P0
- 依存: FEAT-0120
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

`Type ⊑ LType`、U-Bind、データ型の種推論、`Unique`/`freeze`/`unique`、`Cirka.Ownership`。

## 作業内容

- [x] LType と部分種 (述語 `is_ltype`、ADR-038)
- [x] U-Bind・U-Arg
- [x] データ型の種の lub 推論
- [x] `Consumable`/`Dupable`/`Movable` (`Prelude.Linear` のクラスを `LType` に、`Unique` のインスタンス)

## 受け入れ条件

- [x] E0301/E0310 のエラーテスト (`tests/achc/typeck/fail/unique.hs`)
- [x] LType 値に RC 操作がないことをスナップショットで確認 (`tests/achc/mir/unique.hs`、`achc_mir`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 方式は ADR-038 / 03-type-system.md §2.5。`StandaloneKindSignatures` の `type T :: k`、`forall (a :: LType)`、
  クラス引数の `(a :: LType)` を構文に追加した。
- `unique` のコピー (RC>1 のとき) は破壊的更新 (FEAT-0123) の側で行う。それまで値は不変なので観測できる違いはない。
- `Unique` に対するパターンマッチでのフィールドの一意性の伝播 (§4) は、構築子をエクスポートしないので現状は不要。
  FEAT-0123 の配列で必要になれば追加する。
- `return`/`pure` も `Type` の型変数を持つので、線形専用の値を `IO` から返すと E0310 になる。資源を返す `IO` の操作は
  FEAT-0122 (Release) で `LType` の型変数を持つ専用の操作として用意する。

