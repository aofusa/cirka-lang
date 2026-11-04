# FEAT-0041: Core 最適化の強化

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P0
- 依存: FEAT-0033
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

型クラス/トレイトの特殊化 (単相化)、worker/wrapper と unboxing、case-of-case・ジョインポイント。

## 作業内容

- [ ] 特殊化
- [ ] worker/wrapper・unboxing
- [ ] case-of-case・ジョインポイント

## 受け入れ条件

- [ ] 両トラックのベンチで改善を計測し記録
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- CirkaLang v0.0.1 の計測 (FEAT-0207): binary-trees が Rust の 1.60 倍、n-body が 52.9 倍で、主因は `I#`/`F#` の割り当て。
  CirkaLang の Core は正格 (`let!`・正格フィールド) なので、`Int`/`Float` の引数・戻り値の worker/wrapper は正格性解析なしで行える。
