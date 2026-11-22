# FEAT-0092: 正格な配列 (要素を評価してから格納し、読み出しで force しない)

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0091
- 設計: [06-ir.md](../../design/06-ir.md) §3、[00-overview.md](../../design/00-overview.md) ADR-107

## 概要

共通の Core → MIR は、配列の要素を遅延で格納し、読み出しの結果を `force` する (aCHC の `Data.Array` の意味論)。CirkaLang の値は
サンクでないのに、`bench/binary-search` の内側の反復は要素を読むたびに `force` の検査と、`force` が参照を消費するための `dup` / `drop` を
払っていた (MIR: `array_get_unchecked` → `dup` → `force` → `switch` → `drop`)。Core のプログラムの属性 `strict_arrays` を設け、立っていれば
要素を評価してから格納し、読み出しで `force` しない。CirkaLang のフロントエンドが立てる (ADR-107)。

## 作業内容

- [x] 共通 Core の `Program` に `strict_arrays` を加え、CirkaLang の Core への変換で立てる
- [x] Core → MIR: 立っていれば `array_new` の初期値と `array_set*` の値を評価してから格納し、`array_get*` の結果を `force` しない
- [x] 実行テストの見出し `-- max-rc-ops: N` (インタプリタで数えた参照数の操作。-O2)
- [x] テスト: MIR への変換の単体テスト (`cirka-mir` の `strict_arrays_store_values_and_read_without_forcing`: 属性があれば
      格納の前に呼び出しで評価し `mkthunk` も `force` もない、なければ従来どおり)、CirkaLang の実行テスト
      `run/v007/array_reads.cirka` (二分探索と畳み込みで約 5,600 回読む。`Lazy` の要素は読んでも評価されない)
- [x] bench/binary-search ほか全ベンチを計測して記録する

## 受け入れ条件

- [x] CirkaLang の箱の配列の読み出しに `force` と `dup` / `drop` がない
- [x] aCHC の配列は従来どおり要素が遅延 (既存のテストが通る)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `run/v007/array_reads.cirka` の参照数の操作 (インタプリタ、-O2): 要素を遅延で格納すると 16,171 回、正格では 5,019 回
  (残りは配列を作る `generate` の関数の `dup` と、解放の時の要素の `drop`)。見出しは `-- max-rc-ops: 6000`。
- 二分探索の内側の反復の MIR: `array_get_unchecked` → `switch` (`I#`) → `proj` だけになり、`dup` / `force` / `drop` が消えた。
- 計測 (2026-11-21、Docker、変更前のコンパイラで作ったものと交互に 15 回、binary-trees は 5 回。中央値): 全 11 題材で出力は同じ。
  binary-search 0.354 → 0.307 秒 (0.866 倍。Rust の 1.45 倍 → 1.25 倍)。他は 0.97〜1.01 倍で、配列の要素を読む熱いループがない
  (session の 0.94 倍はスレッドの揺らぎで、最小値では 1.06 倍)。
