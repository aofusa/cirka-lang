# FEAT-0232: 洗練型 (部分集合型) と SMT による自動証明

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P0
- 依存: FEAT-0231, FEAT-0050
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

`{x : A | p}` を「値 + 消去される証明」として扱い、証明義務を cirka-smt で自動的に解く。

## 作業内容

- [x] 部分集合型の構文と elaboration
- [x] 証明義務の SMT 問い合わせへの変換
- [x] 反例の表示 (E0501)
- [x] 解けない場合の手動証明へのフォールバック

## 受け入れ条件

- [x] 範囲内 index・非ゼロ除算・ソート済みの例が自動で通る
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 仕様は [spec-v0.0.4.md](../../design/cirkalang/spec-v0.0.4.md) §3、ADR-079。
- `{x : A | p}` は `A` に簡約されるプリミティブ `subsetOf A p`。照合の位置に `primRefine {A} p 番号 e` (実行時は恒等) を付け、
  `cirkalang-check` の `refine.rs` が経路の条件 (`walk.rs`) と仮定のもとで評価・Z3 (`measure.rs` の翻訳を共用) で示す。
- 例: `tests/cirkalang/verify/pass/refine.cirka` (非ゼロ除算・`Data.Array.index` の範囲・`sorted` な組・最小値・帰納法による `fib n >= 0`・`So`)。
  反例は `verify/fail/refine.cirka`。Z3 なしで決まる例は `run/v004/refine.cirka`、`fail/refine/false.cirka`。
- 部分集合型も生成コードに残らない (`proof_erased.cirka` に含め、MIR の同一性を確かめる)。
- 推論した型からは部分集合型を外す (`x + x` に `x` の述語が広がらないように)。
- 手動の証明: Σ 型 `(x ** prf)` の代わりに、結果が `{u : () | P}` の関数・`So`・等式の証明を仮定にする。
- 制限: 高階の位置・線形の型・トレイトのメソッドの型の部分集合型は扱わない (spec §3.5)。

