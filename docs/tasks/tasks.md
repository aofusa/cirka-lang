# タスク一覧

aCHC / CirkaLang のチケット一覧。運用ルールは [AGENTS.md](../../AGENTS.md) §5、
トラック・マイルストーン・版の対応と優先順位は [11-roadmap.md](../design/11-roadmap.md) を参照。

- トラック: **共通基盤** (両試作が共有するバックエンド・ランタイム) / **aCHC** (Haskell 拡張の試作) / **CirkaLang** (プロトタイプ v0.0.x) / **第二段階** (CirkaLang v0.1.0 以降)
- 状態: `todo` / `in-progress` / `done` / `blocked` / `wontfix`
- 優先度: `P0` (マイルストーン完了に必須) / `P1` (重要) / `P2` (後回し可)
- チケットの状態を変えたら、この一覧も同じコミットで更新する。

## 進捗

| トラック | マイルストーン | 完了 / 全体 |
|---|---|---|
| 共通基盤 | M0 基盤整備 | 5 / 5 |
| 共通基盤 | M1 バックエンド基盤 | 7 / 7 |
| 共通基盤 | M2 Perceus | 3 / 3 |
| 共通基盤 | M3 共通 Core と Core→MIR | 4 / 4 |
| 共通基盤 | SX 共通基盤の拡張 (各トラックの必要に応じて) | 0 / 15 |
| aCHC | H1 Haskell 最小フロントエンド | 0 / 6 |
| aCHC | H2 Haskell 実用化 (スコープ縮小版) | 0 / 10 |
| aCHC | H3 所有権拡張 (.chs) | 0 / 9 |
| aCHC | H4 FFI 拡張と評価 | 0 / 2 |
| aCHC | H5 高度な型システム (CirkaLang v0.0.4 の後) | 0 / 5 |
| aCHC | H6 依存型と検証 (CirkaLang v0.0.4 の後) | 0 / 5 |
| aCHC | H7 効果・能力・セッション型 (CirkaLang v0.0.7 の後) | 0 / 5 |
| CirkaLang | CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ | 5 / 8 |
| CirkaLang | CirkaLang v0.0.2 実用 CUI | 0 / 6 |
| CirkaLang | CirkaLang v0.0.3 借用・リージョン・性能 | 0 / 4 |
| CirkaLang | CirkaLang v0.0.4 証明と検証 | 0 / 6 |
| CirkaLang | CirkaLang v0.0.5 並行性・サーバ・syscall | 0 / 4 |
| CirkaLang | CirkaLang v0.0.6 Vulkan compute・数値計算 | 0 / 4 |
| CirkaLang | CirkaLang v0.0.7 効果・能力・セッション型 | 0 / 4 |
| CirkaLang | CX 遠隔環境での GUI 検証 (任意) | 0 / 1 |
| 第二段階 | P2 第二段階 (CirkaLang v0.1.0 以降) | 2 / 4 |

## 共通基盤

### M0 基盤整備

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0001](features/FEAT-0001-cargo-workspace.md) | Cargo ワークスペースとクレート雛形 | done | P0 | - |
| [FEAT-0002](features/FEAT-0002-docker-dev-env.md) | Docker 開発環境と .env 読み込み | done | P0 | 0001 |
| [FEAT-0003](features/FEAT-0003-test-harness.md) | 統合テストハーネス | done | P0 | 0001 |
| [FEAT-0004](features/FEAT-0004-ci-scripts.md) | CI スクリプト | done | P1 | 0003 |
| [FEAT-0005](features/FEAT-0005-diagnostics.md) | 診断基盤 (cirka-diagnostics) | done | P0 | 0001 |

### M1 バックエンド基盤

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0010](features/FEAT-0010-mir-ir.md) | MIR の定義とテキスト形式 | done | P0 | 0005 |
| [FEAT-0011](features/FEAT-0011-mir-verifier.md) | MIR 所有権検証器 | done | P0 | 0010 |
| [FEAT-0012](features/FEAT-0012-mir-interp.md) | MIR インタプリタ | done | P0 | 0010 |
| [FEAT-0013](features/FEAT-0013-runtime-core.md) | ランタイム最小版 | done | P0 | 0001 |
| [FEAT-0014](features/FEAT-0014-llvm-codegen.md) | LLVM IR 生成とリンク | done | P0 | 0010, 0013 |
| [FEAT-0015](features/FEAT-0015-calls-closures-thunks.md) | 末尾呼び出し・クロージャ・サンク | done | P0 | 0014 |
| [FEAT-0016](features/FEAT-0016-m1-baseline-bench.md) | M1 ベースライン性能検証 | done | P1 | 0015, 0011, 0012 |

### M2 Perceus

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0020](features/FEAT-0020-perceus-insertion.md) | Perceus: dup/drop の自動挿入 | done | P0 | 0011 |
| [FEAT-0021](features/FEAT-0021-borrow-inference.md) | 借用推論 (borrowed parameters) | done | P1 | 0020 |
| [FEAT-0022](features/FEAT-0022-reuse-analysis.md) | drop 特殊化と再利用解析 (FBIP) | done | P0 | 0020 |

### M3 共通 Core と Core→MIR

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0030](features/FEAT-0030-core-ir.md) | 共通 Core IR と Core Lint | done | P0 | 0005 |
| [FEAT-0031](features/FEAT-0031-core-to-mir.md) | Core → MIR 変換 | done | P0 | 0030, 0015, 0020 |
| [FEAT-0032](features/FEAT-0032-driver-common.md) | 共通ドライバ (外部ツール・中間生成物・ダンプ) | done | P0 | 0014, 0003 |
| [FEAT-0033](features/FEAT-0033-core-simplifier.md) | Core 簡約器とインライン | done | P1 | 0030 |

### SX 共通基盤の拡張 (各トラックの必要に応じて)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0040](features/FEAT-0040-unwinding.md) | 例外/パニックのアンワインド | todo | P1 | 0031 |
| [FEAT-0041](features/FEAT-0041-core-optimizations.md) | Core 最適化の強化 | done | P0 | 0033 |
| [FEAT-0042](features/FEAT-0042-escape-analysis.md) | エスケープ解析とスタック割り当て | todo | P2 | 0041 |
| [FEAT-0043](features/FEAT-0043-benchmarks.md) | ベンチマーク集と性能レポート | todo | P1 | 0016 |
| [FEAT-0044](features/FEAT-0044-debug-info.md) | DWARF デバッグ情報 | todo | P2 | 0032 |
| [FEAT-0045](features/FEAT-0045-concurrency-runtime.md) | 並行性ランタイム (スレッド・アトミック RC) | todo | P1 | 0031 |
| [FEAT-0046](features/FEAT-0046-async-io-runtime.md) | 非同期 I/O ランタイム (epoll) | todo | P1 | 0045 |
| [FEAT-0047](features/FEAT-0047-ffi-abi.md) | FFI ABI の完成 (構造体・共用体・可変長引数・コールバック) | todo | P1 | 0031 |
| [FEAT-0048](features/FEAT-0048-bindgen.md) | bindgen (libclang による C ヘッダ取り込み) | todo | P1 | 0047 |
| [FEAT-0049](features/FEAT-0049-region-arena.md) | リージョン用アリーナのランタイム | todo | P2 | 0031 |
| [FEAT-0050](features/FEAT-0050-smt-infra.md) | SMT 連携基盤 (cirka-smt) | todo | P1 | 0005 |
| [FEAT-0051](features/FEAT-0051-allocator-tuning.md) | アロケータの改善 | todo | P2 | 0043 |
| [FEAT-0052](features/FEAT-0052-remote-scripts.md) | 遠隔検証スクリプト (scripts/remote.sh) | todo | P2 | 0002 |
| [FEAT-0053](features/FEAT-0053-target-macos.md) | macOS ターゲットへの移植 | todo | P2 | 0052, 0045 |
| [FEAT-0054](features/FEAT-0054-target-windows.md) | Windows ターゲットへの移植 | todo | P2 | 0052, 0045 |


## aCHC

### H1 Haskell 最小フロントエンド

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0100](features/FEAT-0100-hs-lexer-layout.md) | Haskell 字句解析とレイアウト規則 | done | P0 | 0005 |
| [FEAT-0101](features/FEAT-0101-hs-parser.md) | Haskell 構文解析 | done | P0 | 0100 |
| [FEAT-0102](features/FEAT-0102-hs-renamer.md) | Haskell 名前解決とモジュール | done | P0 | 0101 |
| [FEAT-0103](features/FEAT-0103-hs-typeck-hm.md) | Haskell 型推論 (HM・ADT) | done | P0 | 0102, 0030 |
| [FEAT-0104](features/FEAT-0104-hs-desugar.md) | Haskell 脱糖とパターンマッチのコンパイル | done | P0 | 0103 |
| [FEAT-0105](features/FEAT-0105-achc-cli.md) | achc CLI と achc.toml | done | P0 | 0104, 0031, 0032 |

### H2 Haskell 実用化 (スコープ縮小版)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0110](features/FEAT-0110-hs-typeclasses.md) | 型クラスと deriving | todo | P0 | 0105 |
| [FEAT-0111](features/FEAT-0111-cirka-base.md) | cirka-base: Prelude と基本モジュール (縮小版) | todo | P0 | 0110 |
| [FEAT-0112](features/FEAT-0112-hs-strictness.md) | 正格性解析と Strict/StrictData | todo | P0 | 0033 |
| [FEAT-0113](features/FEAT-0113-hs-io-exceptions.md) | IO・IORef・ハンドル・例外 | todo | P0 | 0111 |
| [FEAT-0114](features/FEAT-0114-hs-numerics.md) | 数値型と Integer | todo | P0 | 0111 |
| [FEAT-0115](features/FEAT-0115-hs-records-modules.md) | レコード構文とモジュールの完成 | todo | P1 | 0110 |
| [FEAT-0116](features/FEAT-0116-hs-ffi-h2010.md) | Haskell 2010 FFI (foreign import ccall) | todo | P1 | 0113 |
| [FEAT-0117](features/FEAT-0117-hs-diff-corpus.md) | 差分テスト集 | todo | P1 | 0113, 0003 |
| [FEAT-0118](features/FEAT-0118-hs-extensions.md) | GHC 拡張 (初期分) | todo | P1 | 0110 |
| [FEAT-0119](features/FEAT-0119-hs-compat-measure.md) | 互換性の計測 (GHC testsuite / nofib の一部) | todo | P2 | 0117 |

### H3 所有権拡張 (.chs)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0120](features/FEAT-0120-hs-linear-types.md) | LinearTypes (多重度) | todo | P0 | 0110 |
| [FEAT-0121](features/FEAT-0121-hs-unique-ltype.md) | 種 LType と Unique 型 | todo | P0 | 0120 |
| [FEAT-0122](features/FEAT-0122-hs-drop-release.md) | Drop / Release による RAII | todo | P0 | 0121 |
| [FEAT-0123](features/FEAT-0123-hs-unique-arrays.md) | 一意配列 (Cirka.Array) | todo | P1 | 0121, 0022 |
| [FEAT-0124](features/FEAT-0124-hs-borrowing.md) | 借用 (Ref / MutRef / Borrow モナド) | todo | P1 | 0121 |
| [FEAT-0125](features/FEAT-0125-hs-chs-mode.md) | .chs モード・CirkaOwnership・ownership レベル | todo | P1 | 0122 |
| [FEAT-0126](features/FEAT-0126-hs-cycles.md) | 循環の検出と局所回収器 | todo | P2 | 0112 |
| [FEAT-0127](features/FEAT-0127-hs-regions.md) | リージョン (aCHC) | todo | P2 | 0124, 0049 |
| [FEAT-0128](features/FEAT-0128-hs-linearity-inference.md) | 線形性の推論 (LType 一般化) | todo | P2 | 0121 |

### H4 FFI 拡張と評価

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0130](features/FEAT-0130-hs-extern-block.md) | extern "C" ブロック (aCHC) | todo | P1 | 0116, 0122, 0047 |
| [FEAT-0132](features/FEAT-0132-achc-evaluation.md) | aCHC の評価レポート | todo | P0 | 0117, 0125, 0043 |

### H5 高度な型システム (CirkaLang v0.0.4 の後)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0140](features/FEAT-0140-hs-gadts.md) | GADTs と存在型 | todo | P1 | 0132, 0235 |
| [FEAT-0141](features/FEAT-0141-hs-type-families.md) | 型族 (TypeFamilies) | todo | P1 | 0140 |
| [FEAT-0142](features/FEAT-0142-hs-datakinds-polykinds.md) | DataKinds・PolyKinds・型演算子 | todo | P1 | 0140 |
| [FEAT-0143](features/FEAT-0143-hs-fundeps.md) | FunctionalDependencies と制約ソルバの改良 | todo | P1 | 0110 |
| [FEAT-0144](features/FEAT-0144-hs-typelits.md) | 型レベル Nat / Symbol (GHC.TypeLits 互換) | todo | P1 | 0142, 0141 |

### H6 依存型と検証 (CirkaLang v0.0.4 の後)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0150](features/FEAT-0150-hs-dependent.md) | 依存量化 (foreach / 必須型引数) とシングルトン | todo | P1 | 0144 |
| [FEAT-0151](features/FEAT-0151-hs-refinement.md) | 洗練型 (Liquid Haskell 互換注釈 + .chs 構文) | todo | P1 | 0050, 0105, 0235 |
| [FEAT-0152](features/FEAT-0152-hs-contracts.md) | 契約 (requires / ensures / invariant) | todo | P2 | 0151 |
| [FEAT-0153](features/FEAT-0153-hs-totality.md) | 全域性・終了性の検査と reflection | todo | P2 | 0151 |
| [FEAT-0154](features/FEAT-0154-hs-check-elim.md) | 証明による実行時検査の除去 (aCHC) | todo | P2 | 0151, 0234 |

### H7 効果・能力・セッション型 (CirkaLang v0.0.7 の後)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0160](features/FEAT-0160-hs-capabilities.md) | ケイパビリティ (Cirka.Capability) | todo | P2 | 0263, 0113 |
| [FEAT-0161](features/FEAT-0161-hs-effects.md) | 効果システム (Eff es a) | todo | P1 | 0263, 0142, 0143 |
| [FEAT-0162](features/FEAT-0162-hs-concurrency.md) | aCHC の並行性 (forkIO / MVar) | todo | P2 | 0045, 0113 |
| [FEAT-0163](features/FEAT-0163-hs-session-types.md) | セッション型 (Cirka.Session) | todo | P2 | 0162, 0122, 0262 |
| [FEAT-0164](features/FEAT-0164-achc-evaluation-2.md) | aCHC 後期拡張の評価 | todo | P2 | 0150, 0151, 0161 |


## CirkaLang

### CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0200](features/FEAT-0200-cl-spec-v001.md) | CirkaLang v0.0.1 言語仕様 | done | P0 | - |
| [FEAT-0201](features/FEAT-0201-cl-syntax.md) | CirkaLang 字句・構文解析 | done | P0 | 0005, 0200 |
| [FEAT-0202](features/FEAT-0202-cl-resolve.md) | CirkaLang 名前解決とモジュール | done | P0 | 0201 |
| [FEAT-0203](features/FEAT-0203-cl-elab-qtt.md) | CirkaLang エラボレータ (QTT 型検査) | done | P0 | 0202 |
| [FEAT-0204](features/FEAT-0204-cl-erase-lower.md) | 消去と共通 Core への変換 | done | P0 | 0203, 0030 |
| [FEAT-0205](features/FEAT-0205-cl-cli.md) | cirka CLI と cirka.toml | done | P0 | 0204, 0031, 0032 |
| [FEAT-0206](features/FEAT-0206-cl-prim-min.md) | プリミティブ型と最小ライブラリ | done | P0 | 0205 |
| [FEAT-0207](features/FEAT-0207-cl-release-v001.md) | CirkaLang v0.0.1 リリース | done | P0 | 0206, 0016, 0022 |

### CirkaLang v0.0.2 実用 CUI

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0210](features/FEAT-0210-cl-traits.md) | トレイト (型クラス) と特殊化 | todo | P0 | 0207 |
| [FEAT-0211](features/FEAT-0211-cl-strings-arrays.md) | 文字列と配列 | todo | P0 | 0210, 0022 |
| [FEAT-0212](features/FEAT-0212-cl-raii-errors.md) | RAII (Drop/Release) とエラー処理 | todo | P0 | 0210 |
| [FEAT-0213](features/FEAT-0213-cl-ffi-std.md) | FFI と libc ベースの std (CUI 向け) | todo | P0 | 0212 |
| [FEAT-0214](features/FEAT-0214-cl-packages.md) | パッケージと依存 (cirka.toml) | todo | P1 | 0205 |
| [FEAT-0215](features/FEAT-0215-cl-release-v002.md) | CirkaLang v0.0.2 リリース | todo | P0 | 0211, 0213, 0214 |

### CirkaLang v0.0.3 借用・リージョン・性能

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0220](features/FEAT-0220-cl-borrowing.md) | 借用 | todo | P0 | 0215 |
| [FEAT-0221](features/FEAT-0221-cl-regions.md) | リージョン | todo | P1 | 0220, 0049 |
| [FEAT-0222](features/FEAT-0222-cl-lazy-cycles.md) | 明示的な遅延 (Lazy a) と循環の扱い | todo | P1 | 0215 |
| [FEAT-0223](features/FEAT-0223-cl-release-v003.md) | CirkaLang v0.0.3 リリース | todo | P0 | 0220, 0222, 0041 |

### CirkaLang v0.0.4 証明と検証

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0230](features/FEAT-0230-cl-totality.md) | 全域性・終了性の検査 | todo | P0 | 0223 |
| [FEAT-0231](features/FEAT-0231-cl-proofs-stdlib.md) | 等式・証明と証明付き標準型 | todo | P0 | 0230 |
| [FEAT-0232](features/FEAT-0232-cl-refinement-smt.md) | 洗練型 (部分集合型) と SMT による自動証明 | todo | P0 | 0231, 0050 |
| [FEAT-0233](features/FEAT-0233-cl-contracts.md) | 契約 (requires / ensures / invariant) | todo | P1 | 0232 |
| [FEAT-0234](features/FEAT-0234-cl-check-elim.md) | 証明に基づく実行時検査の除去 | todo | P1 | 0232, 0211 |
| [FEAT-0235](features/FEAT-0235-cl-release-v004.md) | CirkaLang v0.0.4 リリース | todo | P0 | 0233, 0234 |

### CirkaLang v0.0.5 並行性・サーバ・syscall

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0240](features/FEAT-0240-cl-threads.md) | スレッドと共有 | todo | P0 | 0235, 0045 |
| [FEAT-0241](features/FEAT-0241-cl-net-async.md) | ネットワークと非同期 I/O | todo | P0 | 0240, 0046 |
| [FEAT-0242](features/FEAT-0242-cl-os-syscall.md) | OS 層 (syscall・シグナル・プロセス) | todo | P1 | 0213 |
| [FEAT-0243](features/FEAT-0243-cl-release-v005.md) | CirkaLang v0.0.5 リリース | todo | P0 | 0241, 0242 |

### CirkaLang v0.0.6 Vulkan compute・数値計算

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0250](features/FEAT-0250-cl-ffi-complete.md) | FFI の完成と bindgen の利用 | todo | P0 | 0243, 0047, 0048 |
| [FEAT-0251](features/FEAT-0251-cl-vulkan-compute.md) | Vulkan コンピュートシェーダ (Docker / lavapipe) | todo | P0 | 0250 |
| [FEAT-0252](features/FEAT-0252-cl-math-linalg.md) | 数学・線形代数ラッパー | todo | P2 | 0250 |
| [FEAT-0253](features/FEAT-0253-cl-release-v006.md) | CirkaLang v0.0.6 リリース | todo | P0 | 0251 |

### CirkaLang v0.0.7 効果・能力・セッション型

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0260](features/FEAT-0260-cl-capabilities.md) | ケイパビリティ | todo | P1 | 0253 |
| [FEAT-0261](features/FEAT-0261-cl-effects.md) | 効果システムの試作 | todo | P1 | 0260 |
| [FEAT-0262](features/FEAT-0262-cl-session-types.md) | セッション型 | todo | P2 | 0240 |
| [FEAT-0263](features/FEAT-0263-cl-release-v007.md) | CirkaLang v0.0.7 リリース | todo | P0 | 0261 |

### CX 遠隔環境での GUI 検証 (任意)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0290](features/FEAT-0290-cl-remote-gui.md) | 遠隔環境での GUI サンプル (ウィンドウ + 描画) | todo | P2 | 0052, 0053, 0054, 0253 |


## 第二段階

### P2 第二段階 (CirkaLang v0.1.0 以降)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0300](features/FEAT-0300-cl-v010-design.md) | 第一段階の評価と CirkaLang v0.1.0 仕様 | todo | P1 | 0132, 0263 |
| [FEAT-0301](features/FEAT-0301-cl-stdlib-design.md) | CirkaLang 標準ライブラリ設計 | todo | P2 | 0300 |

## バグチケット

| ID | タイトル | 状態 | 重大度 | 関連 |
|---|---|---|---|---|
| [BUG-0001](bugs/BUG-0001-runtime-unit-test-race.md) | ランタイムの単体テストが並列実行で時々異常終了する | done | minor | FEAT-0013 |
| [BUG-0002](bugs/BUG-0002-opt-known-case-field-occurrences.md) | Core 簡約器の case-of-known でフィールドの束縛が消える | done | major | FEAT-0033 |

新しいチケットは `features/_TEMPLATE.md` / `bugs/_TEMPLATE.md` をコピーして作成する。
