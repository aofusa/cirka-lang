# FEAT-0060: ランタイムの高速経路のインライン化 (bitcode と LTO)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P2
- 依存: FEAT-0014, FEAT-0051
- 設計: [07-backend.md](../../design/07-backend.md) §4、[16-performance.md](../../design/16-performance.md) §4

## 概要

`libcirka_rt.a` の関数 (確保の遅い経路・`cirka_force`・文字列や配列のプリミティブ) は生成コードから不透明な呼び出しになる。
ランタイムを LLVM bitcode でも出力し、生成した IR とリンク時に最適化して、小さな関数をインライン化する。

## 作業内容

- [ ] `cirka-runtime` を bitcode でも出力する (Rust と clang の LLVM の版の差を確認する)
- [ ] `-O2` で bitcode と一緒に最適化する経路をドライバに加える (版が合わなければ従来の静的リンク)
- [ ] ベンチで前後を計測する

## 受け入れ条件

- [ ] 全実行テストがリーク 0 のまま通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- 2026-11-26 の実験 (FEAT-0091 の HTTP サーバ、`docs/artifacts/f1027/lto1`〜`lto5`):
  - ランタイムを `RUSTFLAGS="-C relocation-model=pie -C linker-plugin-lto"` で作ると、rustc は ThinLTO の要約付きの bitcode を書く。
    生成した IR と `clang -flto` (lld) でリンクすると、ランタイムは別の ThinLTO のモジュールとしてコンパイルされ、何もインライン化されない。
    要約を外す (`llvm-link in.o -o out.bc`。`llvm-dis | llvm-as` では要約が残る) と、生成コードと同じ full LTO のモジュールに入る。
  - 生成した IR の関数に `"target-cpu"="x86-64"` がないと、ランタイムの関数 (rustc が付ける) は「target features が合わない」として
    インライン化されない。属性を付けても通常のビルドのコードは変わらない。
  - リンク: `-fuse-ld=lld -Wl,--lto-O3 -Wl,-mllvm,-force-ordered-reductions -Wl,-z,keep-text-section-prefix`。HTTP サーバのリンクは
    1.8 秒から 4.2〜5 秒になる。
  - HTTP サーバ: 1 要求の命令数 827 → 815 (`cirka_force`・`cirka_async_events` がインライン化される。`cirka_sock_recv` (費用 945)・
    `cirka_sock_send` (495)・`cirka_async_watch` (370) は閾値 250 を超える)。閾値を 1000 にすると 767 (-7 %)。触れるコードのライン
    (lackey) は 65.8 → 67.3 本 (閾値 1000 で 69.5 本) と増え、スループットは変わらない (f0 に対して 0.994 / 1.005 倍、閾値 1000 で
    0.996 / 1.000 倍。16 / 64 接続、9 回の交互の実行の中央値)。
- 全ベンチ (同じ方法の full LTO と通常のリンク、Rust と順を回して交互に 15 回、binary-trees は 5 回。`docs/artifacts/f1027/lto6`):
  n-body はどの回も約 3 % 速く (中央値の比 0.967)、array-sort はどの回も約 2 % 遅い (1.028)。binary-trees は 5 回のうち 4 回で
  0.5〜3 % 速い (0.944 は外れ値を含む)。cli-grep・mandelbrot・string-split・effects・binary-search は ±1 %、stencil は揺らぎの中。
  spectral-norm (1.120) と session (0.791) は、この計測の間だけ同じバイナリの時間が 2 つの値に分かれた (spectral-norm は 0.34 秒と
  0.54 秒。最終の計測では 15 回のうち 14 回が 0.34 秒) ので判断できない。リンクの時間は 0.8〜2.5 秒から 2.4〜10 秒になる。
- 結論: 確かな効果は n-body の 3 % (binary-trees は小さく揺らぐ) で、array-sort が遅くなり、`-O2` のリンクが数倍かかる。
  ランタイムの熱い経路 (確保・大きさのわかる解放・`errno`) はすでに生成コードでインライン化している (FEAT-1027) ので、
  既定にはしない。続けるなら、array-sort の遅れの原因 (LTO の後段の最適化が生成コードをもう一度最適化する) を調べ、
  ランタイムの関数だけを取り込む形 (生成コードはそのまま) を試す。
