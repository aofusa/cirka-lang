# FEAT-0043: ベンチマーク集と性能レポート

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0016
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

10-testing.md §5 のベンチ (plain.hs / own.chs / main.cirka / Rust) と `bench-report.md`。

## 作業内容

- [ ] 各ベンチの実装
- [ ] 時間と最大 RSS
- [ ] レポート生成

## 受け入れ条件

- [ ] 性能目標 (00-overview G3) の達成状況がレポートに記載
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

