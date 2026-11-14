# FEAT-0233: 契約 (requires / ensures / invariant)

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P1
- 依存: FEAT-0232
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

契約構文を部分集合型に脱糖し、`--contracts=off|check|verify` を実装する。

## 作業内容

- [x] 構文と脱糖
- [x] check モードの実行時検査
- [x] off で生成コードが同一であることの確認

## 受け入れ条件

- [x] 二分探索・不変条件付きデータ型の例が verify で pass
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 仕様は [spec-v0.0.4.md](../../design/cirkalang/spec-v0.0.4.md) §4、ADR-080。
- 脱糖は `cirkalang-syntax` (parser の `contracts` / `refine_last_param` / `refine_result`)。`invariant` の付いた構成子の型は、関数の型シグネチャの後にもう一度検査する (述語がトレイトや関数を使うため)。
- `check`: `cirkalang_check::check_with` が示せなかった義務を返し、`insert_checks` が印を `checkContract` に置き換える。述語の中の `&&` の照合は `let` で変数にしてから下ろす。
- 例: `tests/cirkalang/verify/pass/contracts.cirka` (二分探索・`Range` の不変条件・`clamp`)、`verify/fail/contracts.cirka`、
  `tests/cirkalang/contracts/modes.cirka` (`off` の生成コードが契約なしと同一、`check` の実行時の違反)。
- あわせて、検証の警告 (W0503) が誤りとして扱われていたのを直した。
- 制限: `check` の述語に多重度 0 の変数を使う義務は実行時に検査できない。

