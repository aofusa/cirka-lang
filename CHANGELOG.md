# CHANGELOG

CirkaLang の版ごとの変更を記録する。プロトタイプ (v0.0.x) の間は版間の互換性を保証しない
(`cirka.toml` の `edition` で対象版を明示する)。aCHC は版を持たないので記録しない。

## CirkaLang v0.0.1

最初のプロトタイプ: 最小の正格言語・所有権・ネイティブコード。仕様は [docs/design/cirkalang/spec-v0.0.1.md](docs/design/cirkalang/spec-v0.0.1.md)。

### 追加

- 構文: モジュールと import、`data` (ADT と GADT 形式)、型シグネチャと等式による定義、ラムダ、`let`、`case`、`if`、`do`、
  演算子の定義と結合性宣言、タプル・リストのリテラル、型付きホール `?name` (FEAT-0201, 0202)。
- 型: 依存関数型と Quantitative Type Theory の多重度 (0 / 1 / ω)、暗黙引数とその推論 (パターン単一化)、
  宇宙 `Type` / `LType`、添字付きデータ型 (`Vect n a` など) と依存パターンマッチ、網羅性と到達不能な節の検査 (FEAT-0203)。
- 所有権: 線形変数の使用回数の検査 (E0301–E0306)、`Unique a` と `freeze`、組み込みの Weaken-Drop、
  資源を持たない型の引数の扱い (ADR-030)。
- 消去と共通 Core への変換: 多重度 0 の項・型・証明は実行時に残らない (FEAT-0204)。以降は aCHC と共通の
  Core 最適化・MIR・Perceus (精密 RC と再利用)・LLVM。
- `cirka build/run/check`、`cirka.toml` (`[package]` の `name` / `edition`)、`--dump=rn,tt` (FEAT-0205)。
- Prelude: `Int` `UInt` `Float` `Char` `String` `IO`、`Bool` `Unit` `Nat` `Pair` `List` `Maybe` `Either`、
  算術・比較・ビット演算・文字列・入出力 (`putStrLn` `printInt` `readInt` `argCount` `getArg` …)・リスト関数 (FEAT-0206)。
- ベンチ: `bench/binary-trees/main.cirka`、`bench/n-body/main.cirka` (FEAT-0207)。

### 性能 ([bench-report.md](docs/design/bench-report.md))

- binary-trees (深さ 21): Rust の 1.60 倍。n-body (500 万ステップ): Rust の 52.9 倍。
  原因は `Int`/`Float` のボックス化 (unboxing は FEAT-0041、v0.0.3) と配列の欠如 (FEAT-0211、v0.0.2)。

### 既知の制限

- トレイト (型クラス) がない。`show` などは型ごとの関数 (`showInt` …)、`do` は `IO` 専用 (v0.0.2)。
- `&&` / `||` は普通の関数で両辺を評価する (短絡評価は v0.0.3)。
- 配列と破壊的更新がない (v0.0.2)。n-body は 5 体のリストを毎ステップ作り直す。
- 型の引数は `Type` だけで、`Pair (Unique Int) (Unique Int)` のような LType を入れる多相はできない (宇宙多相は未定)。
- 終了性を検査しない。型の中の計算は燃料 (E1220) で打ち切る (v0.0.4)。
- `panic` / `exit` はアンワインドしない (FEAT-0040)。
- 数値はボックス化されており、算術ごとに割り当てる (上記)。
- 局所的な `where`、ガード、as パターン、レコードがない。
