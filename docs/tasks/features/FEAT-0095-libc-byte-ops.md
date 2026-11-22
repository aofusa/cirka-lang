# FEAT-0095: 文字列のバイト操作を C ライブラリで行う

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0091
- 設計: [05-runtime.md](../../design/05-runtime.md) §2、[16-performance.md](../../design/16-performance.md) §4

## 概要

`bench/string-split` (Rust の 1.37 倍) と `bench/cli-grep` (1.47 倍) を callgrind で比べると、CirkaLang 版の差は
Builder の伸長時のコピー (`cirka$bytes_copy_at`: 1 バイトずつのループで、1 行あたり約 600 命令)、改行の探索
(`cirka$bytes_find_byte_loop`)、部分文字列の照合 (`cirka$bytes_range_eq_loop`) にある。Rust はそれぞれ `memcpy`・
`memchr` (ワード単位)・`memcmp` を使う。

サポートライブラリ (`crates/cirkalang-lower/src/support.core`) のこの 3 つのループを、ランタイムの関数
`cirka_bytes_copy` (`memmove`)・`cirka_bytes_index` (`memchr`)・`cirka_bytes_eq` (`memcmp`) の呼び出しにする。
範囲の境界は関数の入口で 1 回だけ検査し、外れていれば従来の検査付きの読み書きと同じく `panic` ("index out of bounds")
にする (呼び出し側は範囲を切り詰めて渡すので、通常は起きない)。MIR インタプリタにも同じ関数を置く。

## 作業内容

- [x] ランタイム: `crates/cirka-runtime/src/bytes.rs` (3 つの関数と単体テスト)
- [x] MIR インタプリタ: 同じ 3 つの関数
- [x] サポートライブラリ: 3 つのループを関数の呼び出しにする
- [x] テスト: 実行テスト `tests/cirkalang/run/v007/string_bytes.cirka` (ビューの端での一致・部分的な一致・空と長すぎるパターン・スライスの比較・伸長を繰り返す Builder)
- [x] 全ベンチを計測して記録する

## 受け入れ条件

- [x] `bench/string-split` と `bench/cli-grep` の出力が変わらず、速くなる
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 計測 (FEAT-0093 との比較。FEAT-0094 と同じビルドで計測し、この 2 つの題材に展開の写しがないことを LLVM IR で確かめた。
  11 回ずつ交互に実行した中央値の比、負荷 1.5 未満): cli-grep 0.930 (0.278 → 0.258 秒、Rust との比 1.52 → 1.37)、
  string-split 0.936 (0.262 → 0.245 秒、Rust との比 1.41 → 1.24)。出力は変わらない。
- callgrind (cli-grep、20 万行): 3.32 億命令 (Rust は 2.63 億)。`cirka_bytes_index` は 78 万回 (`memchr` まで含めて 1 回約 50 命令)、
  `cirka_bytes_copy` は 80 万回 (`memcpy` まで含めて約 45 命令)、`cirka_bytes_eq` は 40 万回。valgrind の下では確保の領域を
  予約できず `malloc` を使うので、その分 (約 3,500 万命令) は実際の実行にはない。
