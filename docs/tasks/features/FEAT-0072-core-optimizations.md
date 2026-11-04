# FEAT-0072: Core 最適化の強化

- 状態: todo
- マイルストーン: M7 互換性と性能
- 優先度: P0
- 依存: FEAT-0042
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

型クラスの特殊化 (単相化)、worker/wrapper と unboxing、case-of-case、インライン判定の改善。

## 作業内容

- [ ] 特殊化
- [ ] worker/wrapper
- [ ] case-of-case・ジョインポイント
- [ ] インライン判定

## 受け入れ条件

- [ ] ベンチで改善を計測し記録
- [ ] Core Lint・run テストが通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

