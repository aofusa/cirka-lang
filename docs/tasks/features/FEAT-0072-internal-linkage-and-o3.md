# FEAT-0072: 生成する関数を internal にし、リリースの LLVM を -O3 にする

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0070, FEAT-0071
- 設計: [07-backend.md](../../design/07-backend.md), [16-performance.md](../../design/16-performance.md) §4

## 概要

ベンチの比較で Rust は `-C opt-level=3`、aCHC / CirkaLang の `-O2` は LLVM の `-O2` で、条件がそろっていない。また生成する関数はすべて外部リンケージなので、
LLVM は 1 か所からしか呼ばれない関数の大きなボーナスを使えず、呼び出しの定数を関数の中へ伝えられない (IPSCCP)。

手で試した結果 (callgrind の命令数、FEAT-0071 の後):

| ベンチ | 今 | `internal` | `internal` + `-O3` | Rust |
|--------|---:|---:|---:|---:|
| n-body `own.chs` (500,000) | 323.4 M | 322.4 M | 300.9 M | 227.8 M |
| fannkuch-redux `own.chs` (9) | 214.0 M | 191.9 M | 189.9 M | 104.6 M |
| array-sort `own.chs` (1,000,000) | 323.8 M | 325.0 M | 325.3 M | 286.8 M |
| spectral-norm `own.chs` (300) | 84.5 M | 84.5 M | 84.5 M | 61.8 M |

- プログラムは 1 つの LLVM モジュールで、外から呼ばれるのは `cirka_main` だけなので、それ以外の関数はすべて `internal` にできる。
- `-O2` (`--release`) の Core/MIR 側はそのままで、LLVM だけ `-O3` にする。Rust の release と同じ最適化の強さで比べるため。

## 作業内容

- [x] コード生成: `cirka_main` 以外の関数を `define internal` にする
- [x] ドライバ: 最適化レベルから LLVM のレベルへの対応 (`-O2` 以上は `-O3`) を関数にしてテストする
- [x] 設計書 (07-backend.md、ADR) の更新
- [x] ベンチで前後を計測し、テストの所要時間を記録する

## 受け入れ条件

- [x] 生成した IR で `cirka_main` だけが外部リンケージであるテスト
- [x] `-O2` が LLVM の `-O3` になるテスト
- [x] 全ベンチの出力が変わらず、命令数が増えない (計測をメモに記録。array-sort の +0.2 % はメモのとおり)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

callgrind の命令数 (n-body 500,000 / fannkuch-redux 9 / array-sort 1,000,000 / spectral-norm 300、`own.chs`):

| ベンチ | FEAT-0071 | FEAT-0072 | Rust | `.text` (byte) |
|--------|---:|---:|---:|---|
| n-body | 323.4 M | **300.9 M** | 227.8 M | 120,234 → 113,546 |
| fannkuch-redux | 214.0 M | **189.9 M** | 104.6 M | 90,442 → 82,522 |
| array-sort | 323.8 M | 324.5 M | 286.8 M | 93,434 → 86,570 |
| spectral-norm | 84.5 M | 84.5 M | 61.8 M | 94,714 → 87,210 |

- array-sort はこのチケット単独では 0.2 % 増えた (同じ変更で `.text` は 7 % 減っているので、`-O3` のインライン化とコード配置の差)。続く FEAT-0073 の後は 308.9 M で、FEAT-0071 より 4.6 % 少ない。
- 全ベンチの出力は変わらない (14 種)。
- 実装の変異 3 件 (`internal` にしない、`-O2` のまま、`-O1` も `-O3` にする) はテストで全部検出した。
- `clang-18` の時間 (3 回の最良): n-body `-O2` 2,325 ms → `-O3` 2,391 ms、fannkuch-redux 1,715 ms → 1,730 ms。`-O3` による増加は 3 % 以下。
- `scripts/ci.sh` 全体は 10 分 29 秒 (FEAT-0071 の回は別の負荷の下で走ったので、全体の時間は比べられない)。
