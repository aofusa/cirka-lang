# FEAT-0098: ケイパビリティ (Cirka.Capability)

- 状態: todo
- マイルストーン: M9 検証機能 (別軸・優先度低)
- 優先度: P2
- 依存: FEAT-0043
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

能力トークンで副作用の種類を型に表すライブラリと、`capability-safe` モジュール検査。

## 作業内容

- [ ] `Cap e` と `withCapabilities`
- [ ] 能力を取る IO ラッパ (ファイル・ネットワーク・時刻・乱数)
- [ ] `{-# CIRKA capability-safe #-}` で生の IO を禁止する検査
- [ ] ゼロサイズ型として最適化で消えることの確認

## 受け入れ条件

- [ ] 能力を持たない関数がファイルを読めない (型エラー) 例
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

