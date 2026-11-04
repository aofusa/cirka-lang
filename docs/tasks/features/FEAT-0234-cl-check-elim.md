# FEAT-0234: 証明に基づく実行時検査の除去

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P1
- 依存: FEAT-0232, FEAT-0211
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

証明済みの事実で境界検査・ゼロ除算検査・到達不能分岐を除去する (ADR-019)。

## 作業内容

- [ ] 証明済み事実の Core 注釈
- [ ] MIR での検査なしアクセス・unreachable

## 受け入れ条件

- [ ] 配列ベンチで改善を計測し記録
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

