# FEAT-0210: トレイト (型クラス) と特殊化

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0207
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

一貫性のあるトレイト (孤児実装禁止)、辞書渡し + 特殊化、Eq/Ord/Show/Num 等の基本トレイト、導出。

## 作業内容

- [x] trait / impl
- [x] インスタンス解決 (一貫性検査)
- [x] 辞書渡しと特殊化 (0041 と連携)
- [x] derive

## 受け入れ条件

- [x] トレイトの run / fail テスト
- [x] 特殊化で辞書が消えることを確認
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

言語仕様は [spec-v0.0.2.md](../../design/cirkalang/spec-v0.0.2.md) §3、設計判断は ADR-056 / ADR-057。

- 実装: トレイトは `C$Dict` 構築子のデータ型。インスタンス解決は elaborator の保留目標で、単一化の結果を待つ (`crates/cirkalang-elab/src/elab/traits.rs`)。Prelude の算術・比較はトレイトのメソッドになった。`IO` は Core の newtype。
- 共通基盤の変更: 特殊化を固定点まで繰り返し (`MAX_SPEC_ROUNDS`)、`float_dict_fields` で文脈付きインスタンスの辞書をグローバルの構築子に正規化する。正格な `let` のラムダ・既知の値は遅延 `let` と同様に扱う。
- 確認: `tests/cirkalang.rs` の `cirkalang_traits_specialize_away` が、既知の型のプログラムに `$Dict` が残らないこと、総称的なループに箱が残らないことを確かめる。

計測 (x86_64 Linux、Docker 外、3 回の代表値):

| ベンチマーク | 変更前 | 変更後 |
|--------------|--------|--------|
| n-body 5000000 | 4.33 s | 4.12 s |
| mandelbrot 1000 | 0.09 s | 0.09 s |
| spectral-norm 1000 | 0.17 s | 0.17 s |
| binary-trees 21 | 12.0 s | 11.8 s |
| 総称ループ (`Num a, Ord a`) と単相ループ | — | 1.27 s / 1.29 s (同じ) |
