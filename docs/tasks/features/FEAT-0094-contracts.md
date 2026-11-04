# FEAT-0094: 契約 (requires / ensures / invariant) と --contracts

- 状態: todo
- マイルストーン: M9 検証機能 (別軸・優先度低)
- 優先度: P1
- 依存: FEAT-0091
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

Dafny / Verus 風の契約構文を洗練型に脱糖し、`--contracts=off|check|verify` を実装する。

## 作業内容

- [ ] `requires` / `ensures` / `invariant` の構文 (CirkaContracts)
- [ ] 洗練型への脱糖
- [ ] `check` モードの実行時アサーション挿入 (証明済みのものは省略)
- [ ] `off` で生成コードが契約なしと同一であることの確認

## 受け入れ条件

- [ ] 二分探索・Range の不変条件の例が verify で pass
- [ ] check モードで違反が分かりやすいメッセージで停止
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

