# FEAT-0204: 消去と共通 Core への変換

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0203, FEAT-0030
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md), [06-ir.md](../../design/06-ir.md)

## 概要

多重度 0 の項 (型・証明) を消去し、値に依存した型は `Any`/`cast` に落として共通 Core を生成する。正格評価。

## 作業内容

- [x] 消去
- [x] TT → Core (正格 let!、`Lazy` のみサンク)
- [x] Core Lint を通ることの確認

## 受け入れ条件

- [x] 消去後に証明・型が実行時に残らないことをスナップショットで確認
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装: `crates/cirkalang-lower` (`lower(&tt::Program) -> core::Program`)。型の消去は「多重度 0 で、型が種 (`Type` / `… -> Type`) の束縛」を
  Core の `forall` / 型適用に、それ以外の多重度 0 は削除。データ型の Core のパラメータは種を持つパラメータだけ。
  Core で表せない型 (値の変数・計算が止まった型) は `Any` とし、Core の型が一致しない箇所に `cast` を入れる (例: `Vect` の要素は `Any`)。
- 正格評価: TT の `let` は `let!`、関数適用の引数とコンストラクタのフィールドはアトムでなければ `let!` で先に評価する。
- Core の多重度はすべて ω (spec §6 の 4 を更新。暗黙の Drop と Core Lint の線形検査が整合しないため。v0.0.2 で復活)。
- プリミティブは Core のトップレベル関数として生成し、補助関数は `src/support.core` (Core のテキスト形式) を読み込んで合流する。
  ランタイムに `cirka_f64_to_bytes` を追加し、MIR インタプリタにも `cirka_f64_to_bytes` / `sqrt` を追加した。
- Prelude (`lib/cirkalang/core/Prelude.cirka`) をここで作成した (FEAT-0206 で拡充・テスト)。
- テスト: 消去 (`Vect` の添字が消え、要素型が `Any` になる) と、Prelude 全体の変換が Core Lint を通ること。

