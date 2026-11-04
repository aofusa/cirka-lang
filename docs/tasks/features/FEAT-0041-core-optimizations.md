# FEAT-0041: Core 最適化の強化

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P0
- 依存: FEAT-0033
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

型クラス/トレイトの特殊化 (単相化)、worker/wrapper と unboxing、case-of-case・ジョインポイント。

## 作業内容

- [x] 特殊化 → 辞書の特殊化は辞書を作るフロントエンド (FEAT-0110 / FEAT-0210) で実装する。本チケットの時点で両トラックとも型クラス/トレイトがない
- [x] worker/wrapper・unboxing (正格性解析、CPR、正格な箱型フィールドの展開)
- [x] case-of-case・ジョインポイント (失敗継続はデシュガで既に `let` 束縛のジョインポイント。共有される継続は遅延 `let` のまま)

## 受け入れ条件

- [x] 両トラックのベンチで改善を計測し記録 ([bench-report.md](../../design/bench-report.md) の FEAT-0041 節)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- CirkaLang v0.0.1 の計測 (FEAT-0207): binary-trees が Rust の 1.60 倍、n-body が 52.9 倍で、主因は `I#`/`F#` の割り当て。
  CirkaLang の Core は正格 (`let!`・正格フィールド) なので、`Int`/`Float` の引数・戻り値の worker/wrapper は正格性解析なしで行える。
- 結果 (Docker、3 回平均): binary-trees 36.29 s → 12.34 s (Rust 比 1.60 → 0.55)、n-body 20.52 s → 13.67 s (52.9 → 34.7)。
  aCHC の H1 テストの確保回数: `fib.hs` 88,739 → 728、`lazy.hs` 4,131 → 2,199 ほか全件で減少。
- case-of-case で外側の選択肢を複製すると、1 回だけの代入 (出現解析の前提) が最初の複製で消費される。外側に未消費の代入が残る場合は変換しない (`desugar` の `guards_where_and_fallthrough` で検出)。
