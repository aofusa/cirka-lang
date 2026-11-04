# FEAT-0230: 全域性・終了性の検査

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P0
- 依存: FEAT-0223
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

網羅性・終了性 (構造的再帰 + 尺度) を検査し、`total` / `covering` / `partial` を区別する。型・証明に使う関数は total 必須。

## 作業内容

- [ ] 網羅性 (case tree)
- [ ] 構造的再帰の判定 (size-change)
- [ ] 尺度注釈
- [ ] total 指定の伝播

## 受け入れ条件

- [ ] ackermann 等が pass、非停止の例が fail
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

