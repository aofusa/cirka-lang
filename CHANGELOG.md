# CHANGELOG

CirkaLang の版ごとの変更を記録する。プロトタイプ (v0.0.x) の間は版間の互換性を保証しない
(`cirka.toml` の `edition` で対象版を明示する)。aCHC は版を持たないので記録しない。

## CirkaLang v0.0.2

実用 CUI: トレイト、文字列と配列、RAII と `Result`、C FFI と標準ライブラリ、パッケージ、ソース内のテスト。
仕様は [docs/design/cirkalang/spec-v0.0.2.md](docs/design/cirkalang/spec-v0.0.2.md) (v0.0.1 からの差分)。`cirka.toml` の `edition` は `"0.0.2"`。

### 追加

- トレイト: `trait` / `impl` / `deriving` と制約 `C a => T`、高階の型パラメータ、スーパートレイト、既定のメソッド。
  Prelude の算術・比較・`show` はトレイトのメソッドで、数値リテラルは `fromInt`。辞書は全体特殊化で実行時に残らない (FEAT-0210)。
- 所有権と資源: データ型のパラメータを通した線形性の伝播、線形な値を保持する値 (部分適用・アクション) の検査、
  `IO1` (結果が線形なアクション)、`Drop` / `Release` と終了処理の自動挿入、リークの警告 W0308、`ω` のフィールドと `Ur` (FEAT-0212, 0211, BUG-0015)。
- エラー処理: `Result e a` と `do` の文末の `?`、`unwrap` / `expect` などの補助関数 (FEAT-0212)。
- 文字列と配列: `Data.Array` (箱入り、`Unique` でその場の更新)、`Data.Vec` (伸長する配列)、`Data.Array.Unboxed` (トレイト `Unbox`)、
  `Data.String` (複製しないビュー `Str`、分割・検索・数値の解析、`Builder`) (FEAT-0211)。
- モジュール: `import M as X` と `import M (a, b)` (FEAT-0211)。パッケージ: `cirka.toml` のパス依存と名前空間 (FEAT-0214)。
- ソース内のテスト: `test "名前" = …`、`tests where`、`cirka test` (FEAT-0216)。
- C FFI: `extern "C"` ブロック、`Foreign` (C の型・`Ptr` / `OwnedPtr`・C 文字列) (FEAT-0213)。
- 標準ライブラリ `lib/cirkalang/std`: `Std.IO` `Std.Fs` (`File` は `Release` で閉じる) `Std.Env` `Std.Process` `Std.Time`。誤りは `Result IOError a` (FEAT-0213)。
- Prelude: `print` `isJust` `isNothing`、`panic` の結果を線形な型にできる。
- 共通基盤の最適化: 多相関数の静的引数の特殊化、積型の引数の worker/wrapper、`switch` の腕をまたぐ再利用など (FEAT-0079)。
- ベンチ: `bench/array-sort/main.cirka`、`bench/string-split`、`bench/cli-grep`。

### 変更 (互換性のない変更)

- `edition = "0.0.1"` のパッケージは受け付けない (`"0.0.2"` に変える)。
- 型を書かないデータ型のパラメータは `LType` を範囲とし、線形性を伝播する (`List File` は線形)。型を書かない型変数は `Type` で、線形な型での具体化は E0301。
- 線形な値を保持する部分適用・アクションは 1 回しか使えない (E0303 / E0305。BUG-0015)。
- `extern` と `link` は `extern "…"` の位置で文脈キーワード、`trait` `impl` `deriving` は予約語になった。

### 性能 ([bench-report.md](docs/design/bench-report.md) の「CirkaLang v0.0.2」)

- array-sort・mandelbrot・spectral-norm は Rust と同等、binary-trees は Rust の 0.49 倍 (v0.0.1 は 1.60 倍)。
- 文字列処理は Rust の 1.6〜2.0 倍 (string-split・cli-grep)。n-body は 10.3 倍 (v0.0.1 は 52.9 倍)。

### 既知の制限

- `&&` / `||` は両辺を評価する (v0.0.3)。`do` の中の `let` に複数行の `case`、`_ <-`、組のパターン束縛は書けない。
- `panic` / `exit` はアンワインドせず、終了処理を実行しない (FEAT-0040)。
- FFI は 1 つのスカラーで渡せる型だけ。構造体・コールバック・可変長引数・任意のライブラリのリンクはない (FEAT-0047)。
- 借用がなく、線形な値を読むだけの関数も値を返し直す (v0.0.3)。
- case の後の計算が結果を箱で受け取る (結合点がない。FEAT-0080)。レコードの `Float` フィールドは箱 (v0.0.3)。
- 標準入出力・ファイル以外の OS の機能 (ディレクトリ・プロセスの起動・ソケット) はない (v0.0.5)。

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
