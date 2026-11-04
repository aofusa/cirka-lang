# タスク一覧

aCHC / CirkaLang のチケット一覧。運用ルールは [AGENTS.md](../../AGENTS.md) §5、
マイルストーンの目的と終了条件は [11-roadmap.md](../design/11-roadmap.md) を参照。

- 状態: `todo` / `in-progress` / `done` / `blocked` / `wontfix`
- 優先度: `P0` (マイルストーン完了に必須) / `P1` (重要) / `P2` (後回し可)
- チケットの状態を変えたら、この一覧も同じコミットで更新する。

## 進捗

| マイルストーン | 完了 / 全体 |
|---|---|
| M0 基盤整備 | 0 / 5 |
| M1 バックエンド基盤 | 0 / 7 |
| M2 Perceus | 0 / 3 |
| M3 Haskell 最小フロントエンド | 0 / 8 |
| M4 Haskell 実用化 | 0 / 8 |
| M5 線形性・一意性・RAII | 0 / 9 |
| M6 FFI 拡張 | 0 / 4 |
| M7 互換性と性能 | 0 / 10 |
| M8 並行性 | 0 / 2 |
| M9 検証機能 (別軸・優先度低) | 0 / 10 |
| P2 CirkaLang | 0 / 5 |

## 機能チケット

### M0 基盤整備

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0001](features/FEAT-0001-cargo-workspace.md) | Cargo ワークスペースとクレート雛形 | todo | P0 | - |
| [FEAT-0002](features/FEAT-0002-docker-dev-env.md) | Docker 開発環境と .env 読み込み | todo | P0 | 0001 |
| [FEAT-0003](features/FEAT-0003-test-harness.md) | 統合テストハーネス (run / fail / mir / diff) | todo | P0 | 0001 |
| [FEAT-0004](features/FEAT-0004-ci-scripts.md) | CI スクリプト | todo | P1 | 0003 |
| [FEAT-0005](features/FEAT-0005-diagnostics.md) | 診断基盤 (cirka-diagnostics) | todo | P0 | 0001 |

### M1 バックエンド基盤

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0010](features/FEAT-0010-mir-ir.md) | MIR の定義とテキスト形式 | todo | P0 | 0005 |
| [FEAT-0011](features/FEAT-0011-mir-verifier.md) | MIR 所有権検証器 | todo | P0 | 0010 |
| [FEAT-0012](features/FEAT-0012-mir-interp.md) | MIR インタプリタ | todo | P0 | 0010 |
| [FEAT-0013](features/FEAT-0013-runtime-core.md) | ランタイム最小版 | todo | P0 | 0001 |
| [FEAT-0014](features/FEAT-0014-llvm-codegen.md) | LLVM IR 生成とリンク | todo | P0 | 0010, 0013 |
| [FEAT-0015](features/FEAT-0015-calls-closures-thunks.md) | 末尾呼び出し・クロージャ・サンク | todo | P0 | 0014 |
| [FEAT-0016](features/FEAT-0016-m1-baseline-bench.md) | M1 ベースライン性能検証 | todo | P1 | 0015, 0011, 0012 |

### M2 Perceus

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0020](features/FEAT-0020-perceus-insertion.md) | Perceus: dup/drop の自動挿入 | todo | P0 | 0011 |
| [FEAT-0021](features/FEAT-0021-borrow-inference.md) | 借用推論 (borrowed parameters) | todo | P1 | 0020 |
| [FEAT-0022](features/FEAT-0022-reuse-analysis.md) | drop 特殊化と再利用解析 (FBIP) | todo | P0 | 0020 |

### M3 Haskell 最小フロントエンド

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0030](features/FEAT-0030-lexer-layout.md) | 字句解析とレイアウト規則 | todo | P0 | 0005 |
| [FEAT-0031](features/FEAT-0031-parser.md) | 構文解析 (Haskell 2010) | todo | P0 | 0030 |
| [FEAT-0032](features/FEAT-0032-renamer.md) | 名前解決とモジュール | todo | P0 | 0031 |
| [FEAT-0033](features/FEAT-0033-core-ir.md) | Core IR の定義と Core Lint | todo | P0 | 0005 |
| [FEAT-0034](features/FEAT-0034-typeck-hm.md) | 型推論 (HM・ADT・型シグネチャ) | todo | P0 | 0032, 0033 |
| [FEAT-0035](features/FEAT-0035-desugar.md) | 脱糖とパターンマッチのコンパイル | todo | P0 | 0034 |
| [FEAT-0036](features/FEAT-0036-core-to-mir.md) | Core → MIR 変換 | todo | P0 | 0035, 0015, 0020 |
| [FEAT-0037](features/FEAT-0037-driver-cli.md) | achc ドライバと CLI | todo | P0 | 0036, 0003 |

### M4 Haskell 実用化

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0040](features/FEAT-0040-typeclasses.md) | 型クラスと deriving | todo | P0 | 0037 |
| [FEAT-0041](features/FEAT-0041-cirka-base-prelude.md) | cirka-base: Prelude と基本モジュール | todo | P0 | 0040 |
| [FEAT-0042](features/FEAT-0042-core-opt-strictness.md) | Core 最適化の基本と正格性解析 | todo | P0 | 0036 |
| [FEAT-0043](features/FEAT-0043-io-exceptions-abort.md) | IO・IORef・ハンドル・例外 (abort) | todo | P0 | 0041 |
| [FEAT-0044](features/FEAT-0044-numerics.md) | 数値型と Integer | todo | P0 | 0041 |
| [FEAT-0045](features/FEAT-0045-records-modules.md) | レコード構文とモジュールシステムの完成 | todo | P1 | 0040 |
| [FEAT-0046](features/FEAT-0046-ffi-haskell2010.md) | Haskell 2010 FFI (foreign import ccall) | todo | P1 | 0043 |
| [FEAT-0047](features/FEAT-0047-diff-test-corpus.md) | 差分テスト集の整備 | todo | P1 | 0043, 0003 |

### M5 線形性・一意性・RAII

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0050](features/FEAT-0050-linear-types.md) | LinearTypes (多重度) | todo | P0 | 0040 |
| [FEAT-0051](features/FEAT-0051-unique-ltype.md) | 種 LType と Unique 型 | todo | P0 | 0050 |
| [FEAT-0052](features/FEAT-0052-drop-release.md) | Drop / Release による RAII | todo | P0 | 0051 |
| [FEAT-0053](features/FEAT-0053-unique-arrays.md) | 一意配列 (Cirka.Array) と in-place 更新 | todo | P0 | 0051, 0022 |
| [FEAT-0054](features/FEAT-0054-borrowing.md) | 借用 (Ref / MutRef / Borrow モナド) | todo | P1 | 0051 |
| [FEAT-0055](features/FEAT-0055-regions.md) | リージョン | todo | P2 | 0054 |
| [FEAT-0056](features/FEAT-0056-cycles.md) | 循環の検出 (警告) と局所回収器 | todo | P2 | 0042 |
| [FEAT-0057](features/FEAT-0057-linearity-inference.md) | 線形性の推論 (LType 一般化) | todo | P2 | 0051 |
| [FEAT-0058](features/FEAT-0058-chs-mode.md) | .chs モード・CirkaOwnership・ownership レベル | todo | P1 | 0052 |

### M6 FFI 拡張

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0060](features/FEAT-0060-extern-block.md) | extern "C" ブロックと所有権付きポインタ | todo | P1 | 0046, 0052 |
| [FEAT-0061](features/FEAT-0061-bindgen.md) | achc bindgen と import "C" | todo | P2 | 0060 |
| [FEAT-0062](features/FEAT-0062-extern-structs-callbacks.md) | C 構造体とコールバック | todo | P2 | 0060 |
| [FEAT-0063](features/FEAT-0063-math-linalg-wrappers.md) | cirka-math / cirka-linalg 安全ラッパーの試作 | todo | P2 | 0060, 0053 |

### M7 互換性と性能

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0070](features/FEAT-0070-unwinding.md) | 例外のアンワインド | todo | P1 | 0043, 0052 |
| [FEAT-0071](features/FEAT-0071-ghc-testsuite-compat.md) | GHC testsuite / nofib による互換性計測 | todo | P1 | 0047 |
| [FEAT-0072](features/FEAT-0072-core-optimizations.md) | Core 最適化の強化 | todo | P0 | 0042 |
| [FEAT-0073](features/FEAT-0073-escape-analysis.md) | エスケープ解析とスタック割り当て | todo | P2 | 0072 |
| [FEAT-0074](features/FEAT-0074-benchmarks.md) | ベンチマーク集と性能レポート | todo | P1 | 0016, 0041 |
| [FEAT-0075](features/FEAT-0075-ghc-extensions.md) | GHC 拡張の追加 | todo | P1 | 0040 |
| [FEAT-0076](features/FEAT-0076-hackage-packages.md) | Hackage パッケージの移植 | todo | P2 | 0075 |
| [FEAT-0077](features/FEAT-0077-debug-info.md) | DWARF デバッグ情報 | todo | P2 | 0037 |
| [FEAT-0078](features/FEAT-0078-incremental.md) | モジュール単位のキャッシュ | todo | P2 | 0037 |
| [FEAT-0079](features/FEAT-0079-random-diff-fuzz.md) | ランダムプログラム差分テストとファズ | todo | P2 | 0047 |

### M8 並行性

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0080](features/FEAT-0080-concurrency.md) | 並行性 (forkIO / MVar / アトミック RC) | todo | P2 | 0043 |
| [FEAT-0081](features/FEAT-0081-allocator-tuning.md) | アロケータの改善 | todo | P2 | 0074 |

### M9 検証機能 (別軸・優先度低)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0090](features/FEAT-0090-verify-infra.md) | 検証基盤 (cirka-verify・SMT 連携・achc verify) | todo | P1 | 0033, 0037 |
| [FEAT-0091](features/FEAT-0091-refinement-types.md) | 洗練型 (Liquid Haskell 互換注釈 + .chs 構文) | todo | P1 | 0090 |
| [FEAT-0092](features/FEAT-0092-refinement-ownership.md) | 洗練型と所有権・借用の統合 | todo | P2 | 0091, 0053 |
| [FEAT-0093](features/FEAT-0093-liquid-inference.md) | Liquid 型推論 (述語抽象) | todo | P2 | 0091 |
| [FEAT-0094](features/FEAT-0094-contracts.md) | 契約 (requires / ensures / invariant) と --contracts | todo | P1 | 0091 |
| [FEAT-0095](features/FEAT-0095-termination.md) | 終了性・全域性の検査 | todo | P2 | 0090 |
| [FEAT-0096](features/FEAT-0096-proof-based-check-elim.md) | 証明に基づく実行時検査の除去 | todo | P1 | 0091, 0053 |
| [FEAT-0097](features/FEAT-0097-typelevel-nat.md) | 軽量依存型: 型レベル自然数と Cirka.Nat | todo | P2 | 0075 |
| [FEAT-0098](features/FEAT-0098-capabilities.md) | ケイパビリティ (Cirka.Capability) | todo | P2 | 0043 |
| [FEAT-0099](features/FEAT-0099-session-types.md) | セッション型 (Cirka.Session) | todo | P2 | 0080, 0052 |

### P2 CirkaLang

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0100](features/FEAT-0100-cirkalang-design.md) | 第一段階の評価と CirkaLang 言語設計 | todo | P1 | 0071, 0074, 0058 |
| [FEAT-0101](features/FEAT-0101-cirkalang-stdlib-design.md) | CirkaLang 標準ライブラリ設計 | todo | P2 | 0100 |
| [FEAT-0102](features/FEAT-0102-qtt-dependent-core.md) | QTT に基づく依存型 Core の設計 (CirkaLang) | todo | P2 | 0100, 0097 |
| [FEAT-0103](features/FEAT-0103-effect-system-design.md) | 効果システムの設計 (CirkaLang) | todo | P2 | 0100, 0098 |
| [FEAT-0104](features/FEAT-0104-verification-integration.md) | 検証機能の言語統合 (CirkaLang) | todo | P2 | 0100, 0094 |

## バグチケット

| ID | タイトル | 状態 | 重大度 | 関連 |
|---|---|---|---|---|
| - | (なし) | | | |

新しいチケットは `features/_TEMPLATE.md` / `bugs/_TEMPLATE.md` をコピーして作成する。
