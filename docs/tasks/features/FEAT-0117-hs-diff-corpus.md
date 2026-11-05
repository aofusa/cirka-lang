# FEAT-0117: 差分テスト集

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P1
- 依存: FEAT-0113, FEAT-0003
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

GHC と比較する差分テストを 50 本以上 (遅延評価・型クラス・数値・文字列・IO)。

## 作業内容

- [x] `tests/achc/diff/` へのプログラム追加
- [x] 失敗の分類とバグチケット起票

## 受け入れ条件

- [x] 50 本以上が GHC と一致、リーク 0
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 54 本 (遅延評価・アルゴリズム・文字列・ADT とレコード・型クラス・数値・モナド・IO と IORef・Data.List・高階関数・
  小さなシミュレーション・終了コードとエラー・ベンチマークの縮小版・Show/Read・Enum・Bits・テキスト整形)。
  `tests/achc/diff/` に置き、期待値は Docker 内の GHC 9.4.7 (`-O0`) の出力をそのまま使った。すべて一致・リーク 0。
- 作成時に GHC と食い違ったのは 4 本で、いずれも cirka-base の不足だったので本チケットで直した:
  `RealFrac (Ratio a)` / `Enum (Ratio a)` がない、関数の `Semigroup` / `Monoid` がない、`Bits Integer` がない、
  `Double` / `Float` の `enumFromThenTo` が繰り返し加算で誤差が溜まる (GHC は `x + k * step`)。バグチケットに残すものはなかった。
- 実行時間: 1 本あたり数秒 (大半はコンパイラと clang)。差分テストは -O2 だけで回す (-O0 は実行テストで見る)。
  `[profile.dev]` を `opt-level = 1` にして統合テスト全体を速くした (デバッグアサーションとオーバーフロー検査は残る)。

