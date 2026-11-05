# aCHC 互換性計測レポート (前期)

FEAT-0119 の計測結果。目的は合格率の向上ではなく (ADR-025)、**「GC なしで Haskell を実行すると何が問題になるか」** と
**「CirkaLang で何を捨てるか」** を知ること。後期 (H5) の計測 (FEAT-0164) と比べるため、前期の基準としてここに残す。

## 1. 方法

| 項目 | 内容 |
|------|------|
| 対象 (GHC) | GHC 9.4.7 の testsuite のうち `codeGen` / `typecheck` / `deriving` / `numeric` / `simplCore` の `should_run` で、`compile_and_run` (追加ファイル・GHC フラグ・C/Cmm なし) のもの、と `programs/` の複数モジュールのプログラム。計 321 本を `tests/achc/compat/ghc/` に取り込み (BSD-3、各ディレクトリに LICENSE) |
| 対象 (nofib) | `imaginary` / `spectral` のうち期待出力のあるもの 47 本 (引数・入力は "fast" のもの)。nofib は単一のライセンスを持たない (作者ごとの表記、表記のないものもある) ため **リポジトリには入れず**、`CIRKA_NOFIB_DIR` から `_build/compat/nofib` に取り込んで計測する |
| 取り込み | `scripts/import-compat.py` (選別規則は同スクリプトの説明) |
| 実行 | `scripts/compat.sh`。各プログラムを子プロセスで aCHC により -O2 でネイティブにビルドし (リリース版ランタイム)、`CIRKA_LEAK_CHECK=1` で実行して stdout・終了コードを期待値と比べる。制限時間 60 秒 |
| 分類 | 下表。最初の診断で分類し、未対応の拡張 (E0001) があればそれを優先する (構文エラーの原因であることが多いため) |
| 基準 | `tests/achc/compat/baseline.tsv`。前回 `pass` だったものが落ちると計測が失敗する (回帰検出) |

| 分類 | 意味 |
|------|------|
| pass | GHC と同じ出力・終了コード、リークなし |
| parse | 字句・構文エラー (E01xx) |
| extension | 未対応の言語拡張・構文 (E0001) |
| module / name | `cirka-base` にないモジュール / 名前 |
| type | GHC が受理する型・クラスのエラー (E06xx) |
| cyclic | 再帰的な値束縛 (循環データ)。RC では回収できないので拒否する |
| ice | コンパイラの内部エラー・異常終了 |
| crash / output / leak / timeout | 実行時の誤り / 出力の違い / リーク / 時間切れ |

## 2. 結果 (2026-11-05、FEAT-0118 完了時点)

| 分類 | GHC (321) | nofib (47) |
|------|----------:|-----------:|
| pass | 104 | 19 |
| extension | 151 | 2 |
| module | 35 | 11 |
| parse | 11 | 9 |
| cyclic | 3 | 5 |
| type | 6 | 1 |
| output | 6 | 0 |
| crash | 3 | 0 |
| name / other-compile | 1 / 1 | 0 |
| ice / leak / timeout | 0 | 0 |

領域別の pass: codeGen 39/78、numeric 24/46、deriving 15/35、programs 10/23、simplCore 8/42、typecheck 8/97、
nofib imaginary 6/14、spectral 13/33。

### 2.1 未対応の拡張 (153 本)

最多は `MagicHash` (47) で、`GHC.Exts` のプリミティブ (`Int#`、`+#`、配列のプリミオプ) を直接使うテスト。
以下 `ImplicitParams` (11)、`DataKinds` (9)、`CPP` (9)、`UnboxedTuples` (8)、`UndecidableInstances` / `TypeFamilies` /
`FunctionalDependencies` (各 6)、`GADTs` (5 + `GADTSyntax` 4)、`ExistentialQuantification` / `TypeApplications` (各 4)。
`typecheck` の pass が低いのはほぼこれによる (97 本中 81 本が拡張)。

- **H5 で入るもの** (14-achc-advanced.md): GADTs・型族・DataKinds・関数従属・存在型・TypeApplications 等。
  前期との差を FEAT-0164 で記録する。
- **前期で安く入るもの**: `StandaloneDeriving`・`DeriveFunctor/Foldable`・`EmptyDataDeriving`・`ExplicitForAll`・
  `BlockArguments`・`NumDecimals`・`ParallelListComp`・多引数クラスのスーパークラス (`T6117` 等 3 本)・
  入れ子の `forall` の下の制約 (2 本)。
- **入れない方針のもの**: `MagicHash` / `UnboxedTuples` (aCHC は `Cirka.Prim` の `foreign import prim` を使う。GHC の
  プリミオプ名との互換層は作らない)、`CPP`、`ImplicitParams`、`OverlappingInstances` / `UndecidableInstances`
  (インスタンス解決の停止性と一貫性を保つ。後期の検証機能とも相性が悪い)。

### 2.2 ライブラリ (46 本)

`Data.Array` / `Data.Ix` (6)、`Data.Complex` (3)、`Control.Exception` (3)、`System.IO.Unsafe` / `GHC.*` (8)、
`Control.Monad.ST` / `ST.Lazy` (2)、transformers の `State` (3)、nofib 共通の `NofibUtils` (3) など。
配列・`ST`・`IORef` 以外の可変状態は所有権 (`.chs`) で置き換える方針なので、`Data.Array` (不変配列) と `ST` は
H3 (FEAT-0120〜) で所有権の上に実装するのがよい。`unsafePerformIO` は意図的に提供しない。

### 2.3 その他

- **parse (20)**: 文芸的 Haskell (`.lhs`) 9 本、`Integer` リテラルが 128 ビットを超える 4 本、
  `{-# UNPACK #-}` 等の位置 (1)、種 `Constraint` (1) など。`.lhs` と巨大リテラルは前期で対応できる。
- **type (7)**: GHC の `-fdefer-type-errors` 前提 (T7023)、型クラス既定 (`T4528a`)、`deriving` の文脈推論が高階の型引数に
  対して不十分 (`drvrun019`: `Eq (m a)`)、`Integral (Ratio Int)` を要求する nofib `exact-reals` (GHC 側の定義に依存) など。
- **output (6)**: `simplCore` の書き換え規則 (`RULES`) が発火することを確かめるテスト 5 本 (aCHC は `RULES` を無視する) と、
  `T9407` (末尾の改行の違い。調査は後回し)。
- **crash (3)**: BUG-0007 (導出 `Read`) 2 本、BUG-0008 (newtype の構築子パターンが評価してしまう) 1 本。

## 3. GC なし実行についての知見

1. **リークは 0。** 実行できた 123 本はすべてリーク検査を通った。Perceus の RC で通常の Haskell プログラムは回収できる。
2. **問題は循環データに集中する (8 本)。** `ones = 1 : ones`、nofib の `wheel-sieve1/2` の `primes`、`pretty`、`expert`、
   回路シミュレーション (`multiplier`、`jtod_circint`) の信号の再帰定義。いずれも遅延評価による自己参照 (knot-tying) で、
   意味は正しく、RC で回収できないだけである。現状の MIR への変換はこれを拒否しているが、方針 (ADR-014、
   04-memory-model.md §5) は「リーク扱い + 警告 W0401」なので、**FEAT-0126 で W0401 として受理し、
   `--cycles=collect` で回収する** ことを優先度を上げて進める (nofib の 2 割弱がこれで動く見込み)。
   - 回路・ストリームの例は `iterate`・明示的な状態渡しで書き換えられ、CirkaLang (正格、ADR-022) ではそもそも書けない。
     **CirkaLang では循環データを作れない設計を維持する** (循環は `Lazy` と可変参照に限る、12-cirkalang.md)。
3. **性能は個別に差がある。** nofib `atom` は正しい出力だが 13 秒かかる (他の nofib のプログラムは 1 秒前後)。浮動小数点の
   リスト演算で中間リストとサンクの割り当てが多い。リストの融合 (FEAT-0055) と正格性解析の改善の材料とする。
4. **見つかったバグ**: 計測中に BUG-0006 (データ型の種の計算の無限再帰、修正済み)、BUG-0007、BUG-0008 と、
   引数のない型クラスでの型検査器のパニック (FEAT-0118 の退行、修正済み)、`scaleFloat` の巨大な指数での停止 (修正済み) を発見した。

## 4. CirkaLang 設計へのフィードバック

| 観察 | CirkaLang への反映 |
|------|------|
| 循環データの拒否で落ちるのは遅延評価特有の書き方だけ | 正格・循環禁止の方針を維持。共有が要る場合は明示的な RC (`Rc`) と弱参照で書かせる |
| `MagicHash` / `UnboxedTuples` のテストが多い | 非ボックス値は型 (`Int#` 相当の値型) として最初から言語に入れ、特別な構文にしない (v0.0.1 の値型) |
| `ImplicitParams` / `UndecidableInstances` / `OverlappingInstances` | 採用しない。暗黙引数はケイパビリティ (v0.0.7) で、インスタンスの重なりは禁止 |
| 型クラス周りの失敗は拡張の不足が大半で、辞書渡し自体は問題なし | トレイト (型クラス) は辞書渡しで実装する方針を維持 |
| `Data.Array` / `ST` の需要 | 標準ライブラリに所有権ベースの可変配列と、それを包む不変配列を用意する |

## 5. 再計測

```sh
CIRKA_NOFIB_DIR=/path/to/nofib scripts/compat.sh       # 結果: docs/artifacts/compat/
CIRKA_BLESS=1 scripts/compat.sh                        # 変化を確認してから基準を更新
```
