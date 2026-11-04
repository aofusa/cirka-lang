# FEAT-0261: 効果システムの試作

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.7 効果・能力・セッション型
- 優先度: P1
- 依存: FEAT-0260
- 設計: [13-verification.md](../../design/13-verification.md), [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

Koka 風の行多相な効果型と、末尾再開形ハンドラの evidence passing によるコンパイルを試作する。

## 作業内容

- [ ] 効果型と推論
- [ ] ハンドラのコンパイル (evidence passing) を共通 Core 上の変換として実装し、aCHC (FEAT-0161) からも使えるようにする
- [ ] IO / 能力との関係

## 受け入れ条件

- [ ] 効果ハンドラのベンチで通常の関数呼び出しと同等のコスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

