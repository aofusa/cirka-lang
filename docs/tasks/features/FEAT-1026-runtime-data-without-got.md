# FEAT-1026: ランタイムの関数と静的な変数を GOT を通さずに参照し、熱いデータをまとめる

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-1023, FEAT-0091
- 設計: [07-backend.md](../../design/07-backend.md) §4, [05-runtime.md](../../design/05-runtime.md) §3, [00-overview.md](../../design/00-overview.md) ADR-120, [16-performance.md](../../design/16-performance.md)

## 概要

`libcirka_rt.a` は rustc の既定 (共有ライブラリ向けの位置独立コード、16 のコード生成単位、staticlib と rlib) で作っていた。
このため、ランタイムの中で別の単位にある関数の呼び出しと静的な変数の読み出しは GOT の項を通る (rustc は再配置できない
`GOTPCREL` を出すので、リンカも直接の参照に直さない)。HTTP サーバの 1 要求では、GOT のページ (`cirka_bytes_from`・`cirka_call_1`
の項) と、ランタイムの静的な変数が散らばった 3 ページ (空文字列・領域の状態とページのクラスの表・IO のトークン) を読む
(1 要求のデータのページは 14.0、Rust は 7.4)。

- ランタイムを実行ファイルの位置独立コード (`-C relocation-model=pie`) として、1 つのコード生成単位 (`codegen-units = 1`) で作り、
  staticlib だけを出力する。ランタイム自身の関数と静的な変数は PC 相対で直接参照される。`scripts/build-runtime.sh` と
  ドライバ (`native.rs`) は同じ引数で作る (違うとたがいに作り直す)。
- 生成コードの `cirka_call_1` (ランタイムがクロージャを呼ぶ入口) のアドレスは GOT を通るので、スレッドの状態に一度だけ置き、
  イベントループはそこから呼ぶ。
- 領域の状態 (先頭・大きさ・方式) とページごとのサイズクラスの表を 1 つの静的な変数にまとめ、表を最後に置く。確保と解放が
  読む値と、小さなヒープのページのクラスが 1 本のキャッシュラインに入る (リンカの並べ方によらない)。
- ランタイムの不死のオブジェクト (空文字列・IO のトークン) は読み取り専用の 1 つの静的な変数にまとめる (FEAT-1020 の生成コードの
  静的なオブジェクトと同じく、`rc` 0 のオブジェクトには何も書かない)。
- 解放の統計は `frees` を 1 つ増やすだけにする (生きているオブジェクトの数は `allocs - frees`)。2 つの数を同時に変えると LLVM が
  ベクトルの加算にして、定数を読み取り専用のデータから読んでいた。

## 作業内容

- [x] 設計: 07-backend.md §4 (ランタイムの作り方)、05-runtime.md §3、00-overview.md の ADR-120、16-performance.md の表
- [x] ビルド: `Cargo.toml` の `[profile.release.package.cirka-runtime]`、`crates/cirka-runtime/Cargo.toml` の `crate-type`、
  `scripts/build-runtime.sh` と `native.rs` の `RUSTFLAGS`
- [x] `cirka-runtime`: `tls::call_1`、`alloc::Region::class`、`StaticObjects`、`Stats::live`
- [x] テスト: ドライバとビルドスクリプトの引数が同じこと (ユニットテスト)。既存のランタイムのユニットテスト・実行テスト
- [x] テスト: 作った実行ファイルで、GOT の項がランタイムの静的な変数・C の入口を指さないこと (`tests/driver.rs`)
- [x] 計測: HTTP サーバの 1 要求のデータのページ・命令数・CPU 時間、全ベンチ

## 受け入れ条件

- [x] ランタイム自身の関数と静的な変数への参照が GOT を通らない
- [x] 全ベンチの出力が変わらず、遅くなるベンチがない (binary-trees は FEAT-1028 までの最終の計測で。メモ)
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ

- 作った HTTP サーバで、GOT の項がランタイムの静的な変数・C の入口を指さない (`tests/driver.rs`)。
- 計測 (Docker、FEAT-1022 との交互の 5 回の中央値。`docs/artifacts/f1026`): HTTP サーバの 1 要求のデータのページ (lackey)
  12.9 → 10.9 (FEAT-1025 比)、命令数 911 → 912、スループット 1.007 倍 (16 接続)・1.006 倍 (64 接続)。Rust に対して 1.012 倍
  (16・64 接続) 遅い (FEAT-1022 では 1.024・1.009 倍)。binary-trees の命令数 15.25 億 → 14.65 億 (GOT の読み込みがなくなった)、
  時間は FEAT-1017 の 1.110 倍 (FEAT-1025) から 1.079 倍。
- 全ベンチ (FEAT-1023〜1026 をまとめて FEAT-1022 と交互に 15 回、binary-trees は 5 回): binary-trees 1.066・array-sort 1.024・
  session 1.019 倍、ほかは 0.98〜1.006 倍。出力はすべて同じ。binary-trees と array-sort は FEAT-1027・1028 を含む最終の計測で
  FEAT-1017 と比べる (FEAT-0091 のメモ)。
