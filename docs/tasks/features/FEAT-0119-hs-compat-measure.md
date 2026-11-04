# FEAT-0119: 互換性の計測 (GHC testsuite / nofib の一部)

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P2
- 依存: FEAT-0117
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

合格率の向上は目標にせず、「GC なし実行で何が問題になるか」を知るための計測に限る。

## 作業内容

- [ ] 取り込みスクリプト (ライセンス表記保持)
- [ ] 失敗理由の分類
- [ ] `docs/design/compat-report.md`

## 受け入れ条件

- [ ] 失敗理由の分類レポートが CirkaLang 設計へのフィードバックとして記録されている
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

