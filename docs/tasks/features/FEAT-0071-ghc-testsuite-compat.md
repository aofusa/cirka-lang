# FEAT-0071: GHC testsuite / nofib による互換性計測

- 状態: todo
- マイルストーン: M7 互換性と性能
- 優先度: P1
- 依存: FEAT-0047
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

GHC testsuite と nofib の一部を取り込み、合格率と失敗理由を自動集計する。

## 作業内容

- [ ] 取り込みスクリプト (ライセンス表記保持)
- [ ] 失敗理由の分類
- [ ] `docs/design/compat-report.md` の生成
- [ ] 退行検出

## 受け入れ条件

- [ ] 11-roadmap.md の互換性目標を達成
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

