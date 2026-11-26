# FEAT-1011: 大きなオブジェクトを透過的な巨大ページに置く

- 状態: in-progress
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0068, FEAT-0091
- 設計: [05-runtime.md](../../design/05-runtime.md) §3, [00-overview.md](../../design/00-overview.md) ADR-116

## 概要

bench/stencil (100 万要素の `Float` の配列 2 つ、8 MiB ずつ) は、内側のループが Rust と命令単位で同じで (vectorize された
SSE2 の同じ本体、全体の命令数は 4.93 億と 4.91 億)、メモリの帯域で律速し、Rust と同等 (中央値の比 1.004〜1.008) だった。
大きなオブジェクトは `malloc` / `calloc` の `mmap` の塊で、4 KiB のページごとにページフォールト (配列 2 つで約 4,000 回) と、
走査ごとに 4,096 回の TLB の外れが起きる。glibc の `glibc.malloc.hugetlb=1` (大きな塊に `madvise(MADV_HUGEPAGE)`) を付ける
実験では、ページフォールトが 4,026 → 965 回、stencil は 0.924 倍 (Rust の 0.924 倍。Rust にも同じ指定をすると 0.918 倍) になった。

ランタイムが確保する 2 MiB 以上のオブジェクトは、2 MiB 境界に揃った内側の範囲に `madvise(MADV_HUGEPAGE)` を付け、
透過的な巨大ページ (THP) に置く。システムの THP の設定が `madvise` (Ubuntu の既定) でも巨大ページになり、`always` では変わらず、
`never` や THP のないカーネルでは何もしない (`madvise` の失敗は無視する)。

## 作業内容

- [x] 設計: 05-runtime.md §3、ADR-116
- [x] `cirka-runtime`: `malloc` / `calloc` / `realloc` で確保した 2 MiB 以上のオブジェクトの内側の範囲に `MADV_HUGEPAGE`、
  `CIRKA_THP=0` で無効 (`.env.sample`)
- [x] テスト: 範囲の計算の単体テスト、大きな配列を確保したプロセスの写像に巨大ページの指定 (`/proc/self/smaps` の `hg`) が付き、
  `CIRKA_THP=0` では付かないこと、大きな配列を作って読む実行テスト
- [ ] 全ベンチを計測して記録する

## 受け入れ条件

- [x] 2 MiB 以上の配列の内側の範囲に巨大ページの指定が付き、`CIRKA_THP=0` では付かない
- [ ] stencil が速くなり、全ベンチの出力が変わらず遅くなるベンチがない
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ
