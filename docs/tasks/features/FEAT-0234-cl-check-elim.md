# FEAT-0234: 証明に基づく実行時検査の除去

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P1
- 依存: FEAT-0232, FEAT-0211
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

証明済みの事実で境界検査・ゼロ除算検査・到達不能分岐を除去する (ADR-019)。

## 作業内容

- [x] 証明済み事実の Core 注釈
- [x] MIR での検査なしアクセス・unreachable

## 受け入れ条件

- [x] 配列ベンチで改善を計測し記録
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 仕様は [spec-v0.0.4.md](../../design/cirkalang/spec-v0.0.4.md) §5、ADR-081。
- Core: `quot_nz`・`rem_nz`・`array_get_unchecked`、`panic "<unreachable>"` (`UNREACHABLE`)。MIR: `div_nz`・`rem_nz` (`checked()` は `div`・`rem`)、既存の `array_get_unchecked`・`unreachable`。
- CirkaLang: `Data.Array.index`・`divNZ`・`modNZ`・`unreachable` が非公開のプリミティブを使う。`Program::proofs_trusted` (`--contracts=off` 以外) のときだけ検査を除く。
- 健全性のため、`verify` で Z3 が決められなかった義務 (W0503) は実行時の検査にし、`let` の結果の事実は `total` な関数に限った。
- テスト: `cirkalang_proofs_remove_checks` (`search` の読み込みが検査なし、`off` では検査あり)、`division_by_a_proved_divisor` (MIR)、`verify/pass/proved.cirka`。
- 計測 (Docker、hyperfine 10 回、bench/binary-search 引数 1000 = 配列 1000 要素に 400 万回の探索):

  | 版 | 時間 |
  |----|------|
  | 証明あり (`verify`) | 339.6 ms ± 3.8 |
  | 検査あり (`--contracts=off`) | 353.0 ms ± 4.7 |
  | Rust (`-C opt-level=3`) | 242.9 ms ± 22.1 |

  3.8% の改善。100 万要素では 2.98 s / 2.97 s (Rust 1.06 s) で差がなく、箱入りの要素の読み込み (キャッシュミス) が支配的。
  箱なし配列 (`UArray`) の証明付きの読み書きは、要素の箱をなくす方が効果が大きいので、必要になったら別のチケットで扱う。

