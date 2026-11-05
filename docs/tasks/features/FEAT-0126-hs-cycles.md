# FEAT-0126: 循環の検出と局所回収器

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P2
- 依存: FEAT-0112
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

再帰的な値束縛の静的検出 (W0401)、`--cycles=warn|error|collect`、試行削除。

## 作業内容

- [x] 値の `letrec` (knot-tying) を MIR への変換で受理する (サンクを確保してから `thunk_init` で結ぶ。以前は C0200 で拒否)
- [x] W0401 (desugar。局所の再帰的な値束縛。`--cycles=error` でエラー、`collect` では出さない) と `--cycles=` / `achc.toml`
- [x] 循環候補の記録 (`cycle_candidate`: 値の `letrec` のサンクと `MUTVAR`。作成時に根のバッファへ)
- [x] 試行削除 (Bacon–Rajan。ランタイム `cycles.rs` と MIR インタプリタ。ADR-043)
- [x] 互換性計測: W0401 のプログラムは `--cycles=collect` で実行し、通れば `cyclic` に分類

## 受け入れ条件

- [x] `let xs = 1 : xs` で警告、`--cycles=collect` でリーク 0 (`tests/achc/run/h3/{cycles,cycles_warn,cycles_many}.hs`、`achc_cycles_flag`、
      ランタイムの単体テスト `cycles::tests::trial_deletion`、desugar の `recursive_value_warnings`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 互換性計測 (2026-11-05): 以前 `cyclic` で拒否していた 8 本 (GHC の cgrun006・cgrun034・jtod_circint、nofib の wheel-sieve1/2・
  expert・multiplier・pretty) がすべて `--cycles=collect` で GHC と同じ出力・リークなしに動く。基準 (baseline) の分類は不変。
  関数型の束縛を引数なしで定義したもの (`t = x where x x = ..`、rittri) は最適化で関数になるので W0401 を出さない。
- `cycles_many.hs` (循環 3000 個と生き続ける循環): `--cycles=collect` で max_live 7184、`warn` では 65033 (リーク)。
- 局所の値の束縛の型検査結果 (`tast::Bind`) のソース位置が空だったので、束縛の位置を入れた (W0401 の表示に必要)。
- 制限: 根は作成時にしか記録しないので、根でないオブジェクトだけからなる循環は回収しない (aCHC では循環は値の `letrec` と
  `MUTVAR` でしか作れない)。生き続ける根は回収のたびに部分グラフを走査し直す (しきい値を生存数の 2 倍にして償却)。

