# FEAT-0204: 消去と共通 Core への変換

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0203, FEAT-0030
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md), [06-ir.md](../../design/06-ir.md)

## 概要

多重度 0 の項 (型・証明) を消去し、値に依存した型は `Any`/`cast` に落として共通 Core を生成する。正格評価。

## 作業内容

- [ ] 消去
- [ ] TT → Core (正格 let!、`Lazy` のみサンク)
- [ ] Core Lint を通ることの確認

## 受け入れ条件

- [ ] 消去後に証明・型が実行時に残らないことをスナップショットで確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

