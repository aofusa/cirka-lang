# FEAT-0103: Haskell 型推論 (HM・ADT)

- 状態: todo
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0102, FEAT-0030
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

型クラスを除く HM 型推論。種推論・型シグネチャ・SCC・ランク1多相。結果を Core に elaborate する。

## 作業内容

- [ ] 種推論
- [ ] 依存解析と let 多相
- [ ] 型シグネチャの検査
- [ ] 型エラー診断

## 受け入れ条件

- [ ] 型エラーテスト一式
- [ ] 推論結果のスナップショット
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

