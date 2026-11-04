# FEAT-0154: 証明による実行時検査の除去 (aCHC)

- 状態: todo
- トラック: aCHC
- マイルストーン: H6 依存型と検証 (CirkaLang v0.0.4 の後)
- 優先度: P2
- 依存: FEAT-0151, FEAT-0234
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

共通基盤の検査除去 (CirkaLang で実装済み) を aCHC の証明済み事実にも適用する。

## 作業内容

- [ ] aCHC の証明済み事実を Core 注釈に変換

## 受け入れ条件

- [ ] 一意配列のベンチで改善を計測
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

