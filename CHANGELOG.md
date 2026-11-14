# CHANGELOG

CirkaLang の版ごとの変更を記録する。プロトタイプ (v0.0.x) の間は版間の互換性を保証しない
(`cirka.toml` の `edition` で対象版を明示する)。aCHC は版を持たないので記録しない。

## CirkaLang v0.0.4

証明と検証: 関数が止まること、等式、部分集合型と契約を、型と SMT (Z3) で示せる。証明は実行時に消え、示した事実で境界検査などの実行時の検査を除ける。
仕様は [docs/design/cirkalang/spec-v0.0.4.md](docs/design/cirkalang/spec-v0.0.4.md) (v0.0.3 からの差分)。`cirka.toml` の `edition` は `"0.0.4"`。

### 追加

- 全域性: `total` / `covering` / `partial` の宣言、構造的な順序と size-change 原理による終了性、尺度 `decreasing e` と `Int` / `Nat` の引数の自動の尺度 (Z3)。
  型の中で使う関数は全域でなければならない。診断 E0502 / E0504 / E0505 (FEAT-0230)。
- 等式と証明: 命題等価 `x = y` と `Refl`、`sym` / `trans` / `cong`、`rewrite p in e`、`with e of alts`、`Void` / `Dec` / `So`。
  証明は全域で (E0506)、実行時に消える。`core` の `Data.Nat` / `Data.Fin` / `Data.Vect` (FEAT-0231)。
- 部分集合型 `{x : A | p}`: 証明義務を評価と Z3 で示す (反例付きの E0501、決まらないときは W0503)。経路の条件・束縛の部分集合型・
  呼び出しの結果の部分集合型 (レコードのフィールドを含む)・配列と文字列の大きさが 0 以上であることを仮定に使う。
  `{x : A | p} -> B` は `B` でも `x` を使える (FEAT-0232, FEAT-0235)。
- 契約: `requires` / `ensures` とデータ型の `invariant`。`--contracts=verify|check|off` (示せない義務を誤りにする / 実行時に検査する (W0507) / 調べない) (FEAT-0233)。
- 証明による実行時検査の除去: `Data.Array.index` (境界検査)、`divNZ` / `modNZ` (ゼロ除算の検査)、`unreachable`。
  Core に `array_get_unchecked` / `quot_nz` / `rem_nz` と到達しない `panic`、MIR に検査のない除算と `unreachable` (FEAT-0234)。
- 検証済みのサンプル: `tests/cirkalang/verify/pass/sample_sort.cirka` (長さを型に持つ挿入ソートと、不変条件で整列を示す 3 要素のソーティングネットワーク)、
  `sample_array.cirka` (境界検査のない総和・内積・スライド窓)、`sample_parser.cirka` (添字の範囲を契約で保つ再帰下降の構文解析) (FEAT-0235)。
- ベンチ: `bench/binary-search` (契約で境界を示した二分探索)。
- 修正: トレイトのメソッドを使う不変条件の実行時検査の ICE と誤った W1231 (BUG-0017)、検証が `Int` のラップを無視していた (BUG-0018)、
  単一化が作るメタ変数による ICE (BUG-0019)。

### 変更 (互換性のない変更)

- `edition = "0.0.3"` のパッケージは受け付けない (`"0.0.4"` に変える)。
- `decreasing` `rewrite` `with` `requires` `ensures` `invariant` は予約語になった。
- Prelude に証明と検査の名前 (`So` `Oh` `Void` `Dec` `Yes` `No` `Refl` `sym` `trans` `cong` `subsetOf` `divNZ` `modNZ` `unreachable` など) が加わった。
- `total` / `covering` / `partial` は使えるようになった (v0.0.3 までは将来用で E1004)。`total` と宣言した関数は、止まることを示せなければ誤り。`mutual` は引き続き将来用。

### 性能 ([bench-report.md](docs/design/bench-report.md) の「CirkaLang v0.0.4」)

- 証明は実行時に消える: 証明・部分集合型・契約のあるプログラムとないプログラムで MIR が一致する (`cirkalang_proofs_are_erased`、`cirkalang_contracts_modes`)。
  証明を使わない既存の 7 題材は v0.0.3 と同じ LLVM IR を生成する (binary-trees は静的な定数の名前だけが違う)。G3 の達成状況は v0.0.3 のまま。
- 証明による検査の除去: `bench/binary-search` で `search` の境界検査がなくなり、`--contracts=off` より 4〜6% 速い (0.344 s / 0.364 s)。Rust は 0.237 s (1.47 倍)。
- 検証の時間: 検証済みサンプル 3 本の `cirka check` は 0.4〜1.0 s (検証なしは 0.2〜0.25 s)。

### 既知の制限

- 部分集合型は一階の位置だけで照合する。関数の型の中 (`(Int -> {r : Int | r > 0}) -> …`) の部分集合型は検査しない。
- `LType` の値・トレイトのメソッドの部分集合型は扱わない。`check` では、述語の中で多重度 0 の変数を使う義務は実行時に検査できない。
- 型の中のトレイトのメソッドや多相なリテラル (`So (d /= 0)`) は、インスタンスが決まるまで簡約されないことがある。単相の関数を使う。
- `let` で束縛した呼び出しの結果の部分集合型を仮定にできるのは、その関数が `total` のときだけ。
- `Int` はラップするので、範囲の証明には上限・下限の条件が要る (`n + 1 > n` は示せない)。乗算を含む非線形の義務は Z3 が決められないことがある (W0503。`verify` では実行時の検査になる)。
- 述語の中の未定義の名前は暗黙の変数になり、E1210 (推論できない) と報告される。
- タクティク・証明の対話的な構成はない (証明は項として書く)。効果・能力・セッション型は v0.0.7 で扱う。

## CirkaLang v0.0.3

借用・リージョン・性能: 線形な値を手で受け渡さずに貸して使える。循環を含むグラフをリージョンで表せる。遅延の明示と `do` の構文の整理。
仕様は [docs/design/cirkalang/spec-v0.0.3.md](docs/design/cirkalang/spec-v0.0.3.md) (v0.0.2 からの差分)。`cirka.toml` の `edition` は `"0.0.3"`。

### 追加

- 借用 `Data.Borrow`: `borrow` / `borrowIO` と糖衣 `borrow x as a in e`、リージョンで添字付けた `Ref a r` と `Borrow a r` / `BorrowIO a r` のモナド
  (`using` `update` `usingIO` `liftIO`)。持ち出しは型エラー (E1200 とリージョンの注記)。配列 (`lengthB` `readB` `writeB` `modifyB` `swapB`) と
  ファイル (`Std.Fs` の `readB` `readAllB` `writeB`) の借用版の操作 (FEAT-0220)。
- `impl {a : LType} -> C (T a)`: `impl` の型変数の種の宣言 (FEAT-0220)。
- リージョン `Data.Region`: `withRegion`、`RIO r`、不変のハンドル `In r a` と可変の参照 `RefIn r a`。循環するグラフをリーク 0 で解放する (FEAT-0221)。
- 遅延 `Data.Lazy`: `Lazy a`、`delay` `force` `now` `mapLazy` (一度だけ計算する)。`Lazy` を持つトップレベルの値の警告 W0310 (FEAT-0222)。
- `do` のパターンの束縛 `p <- e | q => h` (`_ <-`、組の分解、反駁可能なパターン) (FEAT-0224)。
- 共通基盤の最適化: 状態トークンの worker を newtype が包む任意の状態の関数へ一般化し、借用のモナドのクロージャをなくした。
  構成した積の結果の解析が `cast` 越しの末尾呼び出しを見る (FEAT-0220)。β 簡約の後の既知の構築子の不整合を修正 (BUG-0016)。
- ベンチ: `bench/n-body/main.cirka` を一意な `UArray Float` と借用で書き直した。

### 変更 (互換性のない変更)

- `edition = "0.0.2"` のパッケージは受け付けない (`"0.0.3"` に変える)。
- `&&` / `||` は 2 つの引数に適用したとき短絡する (右辺を必要なときだけ評価する)。
- `Lazy` `Region` `Ref` `Borrow` `BorrowIO` `Arena` `RIO` `In` `RefIn` はプリミティブ型の名前になった (それぞれのモジュールで定義)。

### 性能 ([bench-report.md](docs/design/bench-report.md) の「CirkaLang v0.0.3」)

- G3 (Rust の 1.5 倍以内) は 7 題材中 5 つで達成: array-sort 1.04、mandelbrot 1.06、n-body 1.46 (v0.0.2 は 10.3)、spectral-norm 0.93、binary-trees 0.51。
- 未達: cli-grep 1.91、string-split 1.60 (文字列処理の 1 行あたりの確保。FEAT-0080, FEAT-0082)。
- 借用越しのループは、所有者を手で受け渡すループと確保・参照数の操作の回数が同じ。

### 既知の制限

- 結果が `()` の借用の計算を末尾でない位置で呼ぶと、呼び出しごとに組を確保する (FEAT-0081)。
- `&x` / `&mut x` の構文、関数をまたぐリージョンの推論、入れ子のリージョンはない。リージョンに線形な値は置けない。
- `panic` / `exit` はアンワインドせず、終了処理を実行しない (FEAT-0040)。FFI の構造体・コールバックはない (FEAT-0047)。
- モジュールは名前を再公開しないので、リージョンを使うには `import Data.Borrow (Region)` も要る。

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
