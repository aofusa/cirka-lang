# FEAT-0162: aCHC の並行性 (forkIO / MVar)

- 状態: todo
- トラック: aCHC
- マイルストーン: H7 効果・能力・セッション型 (CirkaLang v0.0.7 の後)
- 優先度: P2
- 依存: FEAT-0045, FEAT-0113
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

共通基盤のスレッド・アトミック RC の上に Control.Concurrent の主要部を実装する。

## 作業内容

- [ ] forkIO / MVar / Chan
- [ ] スレッド間共有時の共有マーク

## 受け入れ条件

- [ ] 並行テストが TSan でエラーなし
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

