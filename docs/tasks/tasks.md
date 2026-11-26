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
| 共通基盤 | SX 共通基盤の拡張 (各トラックの必要に応じて) | 62 / 79 |
| aCHC | H1 Haskell 最小フロントエンド | 6 / 6 |
| aCHC | H2 Haskell 実用化 (スコープ縮小版) | 10 / 10 |
| aCHC | H3 所有権拡張 (.chs) | 9 / 9 |
| aCHC | H4 FFI 拡張と評価 | 2 / 2 |
| aCHC | H5 高度な型システム (CirkaLang v0.0.4 の後) | 0 / 5 |
| aCHC | H6 依存型と検証 (CirkaLang v0.0.4 の後) | 0 / 5 |
| aCHC | H7 効果・能力・セッション型 (CirkaLang v0.0.7 の後) | 0 / 5 |
| CirkaLang | CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ | 8 / 8 |
| CirkaLang | CirkaLang v0.0.2 実用 CUI | 7 / 7 |
| CirkaLang | CirkaLang v0.0.3 借用・リージョン・性能 | 5 / 5 |
| CirkaLang | CirkaLang v0.0.4 証明と検証 | 6 / 6 |
| CirkaLang | CirkaLang v0.0.5 並行性・サーバ・syscall | 5 / 5 |
| CirkaLang | CirkaLang v0.0.6 Vulkan compute・数値計算 | 8 / 8 |
| CirkaLang | CirkaLang v0.0.7 効果・能力・セッション型 | 0 / 4 |
| CirkaLang | CX 遠隔環境での GUI 検証 (任意) | 0 / 1 |
| 第二段階 | P2 第二段階 (CirkaLang v0.1.0 以降) | 0 / 2 |

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
| [FEAT-0043](features/FEAT-0043-benchmarks.md) | ベンチマーク集と性能レポート | done | P1 | 0016 |
| [FEAT-0044](features/FEAT-0044-debug-info.md) | DWARF デバッグ情報 | todo | P2 | 0032 |
| [FEAT-0045](features/FEAT-0045-concurrency-runtime.md) | 並行性ランタイム (スレッド・アトミック RC) | done | P1 | 0031 |
| [FEAT-0046](features/FEAT-0046-async-io-runtime.md) | 非同期 I/O ランタイム (epoll) | done | P1 | 0045 |
| [FEAT-0047](features/FEAT-0047-ffi-abi.md) | FFI ABI の完成 (構造体・共用体・可変長引数・コールバック) | done | P1 | 0031 |
| [FEAT-0048](features/FEAT-0048-bindgen.md) | bindgen (libclang による C ヘッダ取り込み) | done | P1 | 0047 |
| [FEAT-0049](features/FEAT-0049-region-arena.md) | リージョン用アリーナのランタイム | done | P2 | 0031 |
| [FEAT-0050](features/FEAT-0050-smt-infra.md) | SMT 連携基盤 (cirka-smt) | done | P1 | 0005 |
| [FEAT-0051](features/FEAT-0051-allocator-tuning.md) | アロケータの改善 | todo | P2 | 0043 |
| [FEAT-0052](features/FEAT-0052-remote-scripts.md) | 遠隔検証スクリプト (scripts/remote.sh) | todo | P2 | 0002 |
| [FEAT-0053](features/FEAT-0053-target-macos.md) | macOS ターゲットへの移植 | todo | P2 | 0052, 0045 |
| [FEAT-0054](features/FEAT-0054-target-windows.md) | Windows ターゲットへの移植 | todo | P2 | 0052, 0045 |
| [FEAT-0055](features/FEAT-0055-list-fusion.md) | リスト融合 (foldr/build) | todo | P2 | 0112 |
| [FEAT-0056](features/FEAT-0056-ww-call-by-value-local.md) | worker/wrapper の拡張 (値渡しの引数・局所関数) | done | P1 | 0041 |
| [FEAT-0057](features/FEAT-0057-io-result-unboxing.md) | IO の結果の unboxing (bind を通る値の箱を消す) | todo | P2 | BUG-0011 |
| [FEAT-0058](features/FEAT-0058-llvm-optimization-hints.md) | 最適化のヒントを LLVM の属性・メタデータまで運ぶ | done | P1 | 0014, 0021, 0022 |
| [FEAT-0059](features/FEAT-0059-array-element-specialization.md) | 配列要素の単相化 (UElem の特殊化) | done | P1 | 0041, 0056 |
| [FEAT-0060](features/FEAT-0060-runtime-lto.md) | ランタイムの高速経路のインライン化 (bitcode と LTO) | todo | P2 | 0014, 0051 |
| [FEAT-0061](features/FEAT-0061-core-cse-specconstr.md) | 共通部分式除去・ループ不変式の移動・SpecConstr とゼロコストの回帰テスト | done | P1 | 0041 |
| [FEAT-0062](features/FEAT-0062-mir-uniqueness-attrs.md) | 一意性と再利用トークンを MIR の型で区別し引数に nonnull / noalias / readonly を付ける | todo | P2 | 0058 |
| [FEAT-0063](features/FEAT-0063-numeric-loop-codegen.md) | 数値ループの生成コード (libm の組み込み関数・一意性検査のインライン化・要素の TBAA・冗長な一意性検査の除去) | done | P1 | 0058 |
| [FEAT-0064](features/FEAT-0064-fannkuch-unique-arrays.md) | fannkuch-redux の一意な配列版と、読み出しの組・累積引数の箱の除去 | done | P1 | 0041, 0123 |
| [FEAT-0065](features/FEAT-0065-multi-value-returns.md) | 複数の値の返却 (組の結果の CPR) | done | P1 | 0041, 0056 |
| [FEAT-0066](features/FEAT-0066-array-sort-strict-loops.md) | array-sort の差の調査 (要素の箱・ループの force・自己末尾呼び出しのループ化) | done | P1 | 0056, 0063, 0065 |
| [FEAT-0067](features/FEAT-0067-unique-loop-specialization.md) | 一意な配列のループの特殊化 (ループの入口の一意性検査の除去) | done | P1 | 0063, 0066 |
| [FEAT-0068](features/FEAT-0068-array-creation-cost.md) | 配列の作成のコスト (遅い段階の worker/wrapper・`newArray` の初期化の省略・`calloc`) | done | P1 | 0056, 0123 |
| [FEAT-0069](features/FEAT-0069-loop-bounds-check-versioning.md) | ループの境界検査の入口への移動 (帰納的な前提条件によるループのバージョニング) | done | P1 | 0063, 0067 |
| [FEAT-0070](features/FEAT-0070-enter-unique-loops-directly.md) | ループの入口で一意性を確かめ、特殊化した関数へ直接入る | done | P1 | 0067, 0069 |
| [FEAT-0071](features/FEAT-0071-loop-breakers.md) | 相互再帰のループ破りを選び、残りの関数をインライン化する | done | P1 | 0067, 0069 |
| [FEAT-0072](features/FEAT-0072-internal-linkage-and-o3.md) | 生成する関数を internal にし、リリースの LLVM を -O3 にする | done | P1 | 0070, 0071 |
| [FEAT-0073](features/FEAT-0073-cold-copies-of-specialized-loops.md) | 冷たい経路に特殊化した関数の写しを持たせ、ループの入口に定数を伝える | done | P1 | 0070, 0072 |
| [FEAT-0074](features/FEAT-0074-case-of-strict-let-of-comparison.md) | 比較の結果を束縛してから分岐する Core を case-of-case にかける | done | P1 | 0013 |
| [FEAT-0075](features/FEAT-0075-value-only-loop-parameters.md) | ループが読むだけの配列の引数を、入口で値と確かめて反復ごとの force をなくす | done | P1 | 0067, 0070, 0073 |
| [FEAT-0076](features/FEAT-0076-kept-parameters-across-calls.md) | 配列をそのまま返す関数の呼び出しをまたいで一意性の事実を運ぶ | done | P1 | 0067, 0070, 0075, 0077 |
| [FEAT-0077](features/FEAT-0077-strictness-through-newtypes.md) | 正格性解析が newtype のコンストラクタを素通しする | done | P1 | 0112 |
| [FEAT-0078](features/FEAT-0078-unboxed-values-in-speculation.md) | 評価済みの引数の判定で unboxed の値を値として数える | done | P1 | 0056, 0068 |
| [FEAT-0079](features/FEAT-0079-boxes-in-string-loops.md) | 文字列処理のループの箱の除去 (多相の静的引数・積型の worker/wrapper・腕をまたぐ再利用) | done | P1 | 0022, 0056, 0061, 0066 |
| [FEAT-0080](features/FEAT-0080-join-points.md) | Core の結合点 (join point) | todo | P2 | 0079 |
| [FEAT-0081](features/FEAT-0081-cpr-single-boxed-result.md) | 箱入りの値 1 つだけを返す積の結果の分解 | todo | P2 | 0065, 0220 |
| [FEAT-0082](features/FEAT-0082-cl-cli-grep-line-allocs.md) | 行ごとの畳み込みの確保の除去 (cli-grep) | done | P2 | 0223 |
| [FEAT-0083](features/FEAT-0083-cheap-speculation-of-lazy-bindings.md) | 安価で失敗しない遅延束縛の先行評価 (サンクの除去) | done | P1 | 0056, 0061 |
| [FEAT-0084](features/FEAT-0084-tsan-in-docker-ci.md) | Docker の CI で ThreadSanitizer を動かす | done | P2 | 0045 |
| [FEAT-0085](features/FEAT-0085-code-alignment-noise.md) | コード配置による性能の揺らぎの抑制 (関数・ループの整列) | todo | P2 | 0043 |
| [FEAT-0086](features/FEAT-0086-cl-server-request-allocs.md) | サーバの要求ごとの確保を減らす (継続・Result・文字列のビュー) | todo | P1 | 0243 |
| [FEAT-0087](features/FEAT-0087-simplifier-convergence.md) | 深く入れ子の式で Core の単純化を早く収束させる | todo | P2 | 0255 |
| [FEAT-0088](features/FEAT-0088-io-action-under-case-closure.md) | 場合分けの後の IO の動作をクロージャにしない (状態トークンの適用を結合点の中に押し込む) | done | P2 | 0252 |
| [FEAT-0089](features/FEAT-0089-static-object-rc.md) | 静的なオブジェクト (引数のない構成子・文字列リテラル) の dup/drop の除去 | done | P2 | BUG-0034 |
| [FEAT-0090](features/FEAT-0090-case-of-case-linear.md) | 線形な値を返す case の case-of-case (`Std.Session` の `offer` の確保) | done | P2 | FEAT-0262 |
| [FEAT-0091](features/FEAT-0091-beat-rust.md) | Rust を上回る性能 (Rust より遅い題材の原因の調査と改善) | in-progress | P1 | 0263 |
| [FEAT-0092](features/FEAT-0092-strict-arrays.md) | 正格な配列 (要素を評価してから格納し、読み出しで force しない) | done | P1 | 0091 |
| [FEAT-0093](features/FEAT-0093-exhaustive-switch.md) | すべての構成子を覆う switch の既定の腕をなくす | done | P1 | 0091 |
| [FEAT-0094](features/FEAT-0094-literal-loop-unrolling.md) | リテラルの範囲のループの展開 (固定の回数のループを添字が定数の直線のコードにする) | done | P1 | 0091 |
| [FEAT-0095](features/FEAT-0095-libc-byte-ops.md) | 文字列のバイト操作 (コピー・バイトの探索・範囲の比較) を C ライブラリで行う | done | P1 | 0091 |
| [FEAT-0096](features/FEAT-0096-resize-in-place.md) | 大きな文字列をその場で伸縮する (Builder の伸長と build、ファイルの読み込み) | done | P1 | 0091 |
| [FEAT-0097](features/FEAT-0097-inline-byte-ops.md) | バイト列のコピー・検索・比較をインラインで生成する (文字列リテラルの長さの畳み込みと切り詰めのない書き込み) | done | P1 | 0091, 0095 |
| [FEAT-0098](features/FEAT-0098-hoist-checks-of-large-loops.md) | 大きなループの本体の境界検査も入口へ移す (解析の上限を命令数 × 解析の回数で決める) | done | P1 | 0069, 0094 |
| [FEAT-0099](features/FEAT-0099-spin-before-sleep.md) | 同期オブジェクトを待つスレッドは眠る前に変化を見張る (起こす側は mutex を放してから起こす) | done | P1 | 0262 |
| [FEAT-1000](features/FEAT-1000-llvm-23.md) | バックエンドの LLVM を Rust と同じ 23 に上げる (apt.llvm.org のパッケージを Docker イメージの最後の層で入れる) | done | P1 | 0091 |
| [FEAT-1001](features/FEAT-1001-edge-triggered-event-loop.md) | イベントループの待ちをエッジトリガーで登録したままにし、記述子ごとに準備の状態を覚える (空にしたソケットの読み込みと待ちにシステムコールを使わない) | done | P1 | 0091, 0241 |
| [FEAT-1002](features/FEAT-1002-io-fixed-costs.md) | ランタイムの I/O の固定費を減らす (スレッドごとの受信バッファ、空の結果は静的な空文字列) | done | P1 | 0091 |
| [FEAT-1003](features/FEAT-1003-concat-with-empty.md) | 文字列の連結の片方が空なら他方を返し、空の切り出しは静的な空文字列にする | done | P1 | 0091, 0097 |
| [FEAT-1004](features/FEAT-1004-constants-are-not-cafs.md) | トップレベルの定数を使う箇所で値に置き換え、CAF の force をなくす (リテラルはどこでも、リテラルの構築子は分解する箇所で) | done | P1 | 0091 |
| [FEAT-1005](features/FEAT-1005-one-latch-per-loop.md) | 自己末尾呼び出しのループの後ろ向きの辺を 1 つのラッチにまとめる (LLVM がループを入れ子に分けない) | done | P1 | 0066, 0091 |
| [FEAT-1006](features/FEAT-1006-static-constant-constructors.md) | 定数だけを受け取る構築子のトップレベルの定義を静的なオブジェクトにする (CAF にせず、force も確保もしない) | done | P1 | 0089, 1004 |
| [FEAT-1007](features/FEAT-1007-raw-io-syscalls.md) | I/O のシステムコールを libc を通さずに呼ぶ (マルチスレッドのプロセスで取り消しの点のアトミックな操作をなくす) | done | P1 | 1001, 0091 |
| [FEAT-1008](features/FEAT-1008-sum-results.md) | 和型の構築子を返す関数は構築子の番号とフィールドを箱なしで返す (和の CPR) | done | P1 | 0065, 0091 |
| [FEAT-1009](features/FEAT-1009-product-joins.md) | 分岐が作ってすぐ分解する積の値は、フィールドを合流点へ渡す (積の合流点) | done | P1 | 0065, 0074, 0091 |
| [FEAT-1010](features/FEAT-1010-ordered-fp-reductions.md) | 浮動小数点の縮約のループを、加算の順序を保ったままベクトル化する | done | P1 | 1000, 0091 |
| [FEAT-1011](features/FEAT-1011-huge-pages-for-large-objects.md) | 大きな配列を透過的な巨大ページに置く | done | P1 | 0068, 0091 |
| [FEAT-1012](features/FEAT-1012-boxed-sum-results.md) | 和型の構築子を返す関数は、箱のフィールドもそのまま返す | done | P1 | 1008, 0091 |
| [FEAT-1013](features/FEAT-1013-recv-stream-sockets.md) | ストリームソケットの受信は `recv` で行う | done | P1 | 1005, 0091 |
| [FEAT-1014](features/FEAT-1014-lighter-callbacks.md) | イベントループのコールバックごとのランタイムの処理を減らす | done | P1 | 1005, 0091 |
| [FEAT-1015](features/FEAT-1015-short-apply-entry.md) | クロージャの呼び出しの、引数の数が合う場合を短い入口にする | done | P1 | 0091 |
| [FEAT-1016](features/FEAT-1016-socket-receive-fast-paths.md) | ソケットの受信の短い経路 (`errno` は空の結果だけ、空とわかっている受信は短い入口) | done | P1 | 1001, 1013, 0091 |
| [FEAT-1017](features/FEAT-1017-static-objects-inline-rc.md) | 静的なオブジェクトの `dup` / `drop` を関数の外のブロックへ飛ばさずに済ませる | done | P1 | 0089, 1006, 0091 |
| [FEAT-1018](features/FEAT-1018-loop-alignment.md) | ループの先頭を 32 / 64 バイトに揃えるかを調べる (採らない) | done | P2 | 1017, 0091 |
| [FEAT-1019](features/FEAT-1019-inline-byte-search.md) | 文字列のバイトの探索を C ライブラリの `memchr` を呼ばずに行う | done | P1 | 0097, 0091 |
| [FEAT-1020](features/FEAT-1020-static-objects-as-constants.md) | 静的なオブジェクトを LLVM の定数にする | done | P1 | 1006, 1017, 0091 |
| [FEAT-1021](features/FEAT-1021-errno-address-in-tls.md) | `errno` のアドレスを TLS ブロックに覚える | done | P1 | 1016, 0091 |
| [FEAT-1022](features/FEAT-1022-short-byte-copies.md) | バッファから作る短い文字列の写しをインラインで行う | done | P1 | 1002, 0091 |
| [FEAT-1023](features/FEAT-1023-hot-runtime-sections.md) | 演算ごとに動くランタイムの関数をテキストの先頭にまとめる | done | P1 | 1019, 1022, 0091 |
| [FEAT-1024](features/FEAT-1024-lighter-loop-turns.md) | イベントループの 1 回りで、使わない受け箱・タイマーの処理を飛ばす | done | P1 | 1014, 0091 |


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
| [FEAT-0110](features/FEAT-0110-hs-typeclasses.md) | 型クラスと deriving | done | P0 | 0105 |
| [FEAT-0111](features/FEAT-0111-cirka-base.md) | cirka-base: Prelude と基本モジュール (縮小版) | done | P0 | 0110 |
| [FEAT-0112](features/FEAT-0112-hs-strictness.md) | 正格性解析と Strict/StrictData | done | P0 | 0033 |
| [FEAT-0113](features/FEAT-0113-hs-io-exceptions.md) | IO・IORef・ハンドル・例外 | done | P0 | 0111 |
| [FEAT-0114](features/FEAT-0114-hs-numerics.md) | 数値型と Integer | done | P0 | 0111 |
| [FEAT-0115](features/FEAT-0115-hs-records-modules.md) | レコード構文とモジュールの完成 | done | P1 | 0110 |
| [FEAT-0116](features/FEAT-0116-hs-ffi-h2010.md) | Haskell 2010 FFI (foreign import ccall) | done | P1 | 0113 |
| [FEAT-0117](features/FEAT-0117-hs-diff-corpus.md) | 差分テスト集 | done | P1 | 0113, 0003 |
| [FEAT-0118](features/FEAT-0118-hs-extensions.md) | GHC 拡張 (初期分) | done | P1 | 0110 |
| [FEAT-0119](features/FEAT-0119-hs-compat-measure.md) | 互換性の計測 (GHC testsuite / nofib の一部) | done | P2 | 0117 |

### H3 所有権拡張 (.chs)

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0120](features/FEAT-0120-hs-linear-types.md) | LinearTypes (多重度) | done | P0 | 0110 |
| [FEAT-0121](features/FEAT-0121-hs-unique-ltype.md) | 種 LType と Unique 型 | done | P0 | 0120 |
| [FEAT-0122](features/FEAT-0122-hs-drop-release.md) | Drop / Release による RAII | done | P0 | 0121 |
| [FEAT-0123](features/FEAT-0123-hs-unique-arrays.md) | 一意配列 (Cirka.Array) | done | P1 | 0121, 0022 |
| [FEAT-0124](features/FEAT-0124-hs-borrowing.md) | 借用 (Ref / MutRef / Borrow モナド) | done | P1 | 0121 |
| [FEAT-0125](features/FEAT-0125-hs-chs-mode.md) | .chs モード・CirkaOwnership・ownership レベル | done | P1 | 0122 |
| [FEAT-0126](features/FEAT-0126-hs-cycles.md) | 循環の検出と局所回収器 | done | P2 | 0112 |
| [FEAT-0127](features/FEAT-0127-hs-regions.md) | リージョン (aCHC) | done | P2 | 0124, 0049 |
| [FEAT-0128](features/FEAT-0128-hs-linearity-inference.md) | 線形性の推論 (LType 一般化) | done | P2 | 0121 |

### H4 FFI 拡張と評価

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0130](features/FEAT-0130-hs-extern-block.md) | extern "C" ブロック (aCHC) | done | P1 | 0116, 0122, 0047 |
| [FEAT-0132](features/FEAT-0132-achc-evaluation.md) | aCHC の評価レポート | done | P0 | 0117, 0125, 0043 |

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
| [FEAT-0210](features/FEAT-0210-cl-traits.md) | トレイト (型クラス) と特殊化 | done | P0 | 0207 |
| [FEAT-0211](features/FEAT-0211-cl-strings-arrays.md) | 文字列と配列 | done | P0 | 0210, 0022 |
| [FEAT-0212](features/FEAT-0212-cl-raii-errors.md) | RAII (Drop/Release) とエラー処理 | done | P0 | 0210 |
| [FEAT-0213](features/FEAT-0213-cl-ffi-std.md) | FFI と libc ベースの std (CUI 向け) | done | P0 | 0212 |
| [FEAT-0214](features/FEAT-0214-cl-packages.md) | パッケージと依存 (cirka.toml) | done | P1 | 0205 |
| [FEAT-0216](features/FEAT-0216-cl-inline-tests.md) | ソース内のテスト (test 宣言と cirka test) | done | P1 | 0207 |
| [FEAT-0215](features/FEAT-0215-cl-release-v002.md) | CirkaLang v0.0.2 リリース | done | P0 | 0211, 0213, 0214, 0216 |

### CirkaLang v0.0.3 借用・リージョン・性能

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0220](features/FEAT-0220-cl-borrowing.md) | 借用 | done | P0 | 0215 |
| [FEAT-0221](features/FEAT-0221-cl-regions.md) | リージョン | done | P1 | 0220, 0049 |
| [FEAT-0222](features/FEAT-0222-cl-lazy-cycles.md) | 明示的な遅延 (Lazy a) と循環の扱い | done | P1 | 0215 |
| [FEAT-0223](features/FEAT-0223-cl-release-v003.md) | CirkaLang v0.0.3 リリース | done | P0 | 0220, 0221, 0222, 0224, 0041 |
| [FEAT-0224](features/FEAT-0224-cl-do-patterns.md) | do の構文の整理 (パターンの束縛) | done | P1 | 0215 |

### CirkaLang v0.0.4 証明と検証

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0230](features/FEAT-0230-cl-totality.md) | 全域性・終了性の検査 | done | P0 | 0223 |
| [FEAT-0231](features/FEAT-0231-cl-proofs-stdlib.md) | 等式・証明と証明付き標準型 | done | P0 | 0230 |
| [FEAT-0232](features/FEAT-0232-cl-refinement-smt.md) | 洗練型 (部分集合型) と SMT による自動証明 | done | P0 | 0231, 0050 |
| [FEAT-0233](features/FEAT-0233-cl-contracts.md) | 契約 (requires / ensures / invariant) | done | P1 | 0232 |
| [FEAT-0234](features/FEAT-0234-cl-check-elim.md) | 証明に基づく実行時検査の除去 | done | P1 | 0232, 0211 |
| [FEAT-0235](features/FEAT-0235-cl-release-v004.md) | CirkaLang v0.0.4 リリース | done | P0 | 0233, 0234 |

### CirkaLang v0.0.5 並行性・サーバ・syscall

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0240](features/FEAT-0240-cl-threads.md) | スレッドと共有 | done | P0 | 0235, 0045 |
| [FEAT-0241](features/FEAT-0241-cl-net-async.md) | ネットワークと非同期 I/O | done | P0 | 0240, 0046 |
| [FEAT-0242](features/FEAT-0242-cl-os-syscall.md) | OS 層 (syscall・シグナル・プロセス) | done | P1 | 0213 |
| [FEAT-0244](features/FEAT-0244-cl-core-size-contracts.md) | core の配列・文字列の関数に大きさの契約を付ける | done | P2 | 0235 |
| [FEAT-0243](features/FEAT-0243-cl-release-v005.md) | CirkaLang v0.0.5 リリース | done | P0 | 0241, 0242 |

### CirkaLang v0.0.6 Vulkan compute・数値計算

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0250](features/FEAT-0250-cl-ffi-complete.md) | FFI の完成と bindgen の利用 | done | P0 | 0243, 0047, 0048 |
| [FEAT-0251](features/FEAT-0251-cl-vulkan-compute.md) | Vulkan コンピュートシェーダ (Docker / lavapipe) | done | P0 | 0250, 0256 |
| [FEAT-0252](features/FEAT-0252-cl-math-linalg.md) | 数学・線形代数ラッパー | done | P2 | 0250, 0256 |
| [FEAT-0257](features/FEAT-0257-cl-linalg-zero-copy.md) | Linalg の BLAS / LAPACK に配列の領域を直接渡す (コピーとバッファの除去) | done | P2 | 0252 |
| [FEAT-0254](features/FEAT-0254-cl-unique-array-sizes.md) | 一意な配列と文字列のビューの大きさを証明で追う | done | P2 | 0244 |
| [FEAT-0255](features/FEAT-0255-cl-large-extern-modules.md) | 大きな extern のモジュールのコンパイル時間 | done | P1 | 0250 |
| [FEAT-0256](features/FEAT-0256-cl-private-constructors.md) | 非公開の構成子 (抽象データ型) | done | P0 | 0250 |
| [FEAT-0253](features/FEAT-0253-cl-release-v006.md) | CirkaLang v0.0.6 リリース | done | P0 | 0251 |

### CirkaLang v0.0.7 効果・能力・セッション型

| ID | タイトル | 状態 | 優先度 | 依存 |
|---|---|---|---|---|
| [FEAT-0260](features/FEAT-0260-cl-capabilities.md) | ケイパビリティ | done | P1 | 0253 |
| [FEAT-0261](features/FEAT-0261-cl-effects.md) | 効果システムの試作 | done | P1 | 0260 |
| [FEAT-0262](features/FEAT-0262-cl-session-types.md) | セッション型 | done | P2 | 0240 |
| [FEAT-0263](features/FEAT-0263-cl-release-v007.md) | CirkaLang v0.0.7 リリース | done | P0 | 0261 |

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
| [BUG-0003](bugs/BUG-0003-codegen-closure-stub-captures.md) | 捕捉数の異なる同一関数のクロージャが入口スタブを共有する | done | critical | FEAT-0110 |
| [BUG-0004](bugs/BUG-0004-exhaustiveness-var-column.md) | 変数を含む列の網羅性検査が誤って W0610 を出す | done | minor | FEAT-0110 |
| [BUG-0005](bugs/BUG-0005-pattern-failure-message.md) | パターン照合の失敗の実行時エラーが GHC の文言・位置と異なる | todo | minor | FEAT-0104 |
| [BUG-0006](bugs/BUG-0006-lint-recursive-kind.md) | 関数型を介して自身を含むデータ型で Core Lint がスタックオーバーフローする | done | major | FEAT-0119 |
| [BUG-0007](bugs/BUG-0007-derived-read-gaps.md) | 導出した Read が演算子のフィールド名と逆引用符の中置構築子を読めない | todo | minor | FEAT-0119 |
| [BUG-0008](bugs/BUG-0008-newtype-pattern-forces.md) | newtype の構築子パターンが値を評価してしまう | done | major | FEAT-0119 |
| [BUG-0009](bugs/BUG-0009-strict-linear-tuple.md) | タプルの成分の線形専用値が正格に評価されない | done | major | FEAT-0123 |
| [BUG-0010](bugs/BUG-0010-test-header-after-pragma.md) | テストのヘッダが `{-# LANGUAGE #-}` の後にあると読まれない | done | major | FEAT-0125 |
| [BUG-0011](bugs/BUG-0011-borrow-monad-overhead.md) | Borrow モナドのループが線形な受け渡しより約 130 倍遅い | done | major | FEAT-0043 |
| [BUG-0012](bugs/BUG-0012-dead-join-undefined-param.md) | どこからも飛ばれない合流ブロックの引数が LLVM IR で未定義になる | done | major | BUG-0011 |
| [BUG-0013](bugs/BUG-0013-cl-do-last-poly-io.md) | CirkaLang の do の最後の文に exit などの多相な IO を書くと型エラーになる | done | minor | FEAT-0203 |
| [BUG-0014](bugs/BUG-0014-lazy-con-strict-fields.md) | 遅延束縛の構築子の正格フィールドが束縛の時点で評価される | done | major | BUG-0008 |
| [BUG-0015](bugs/BUG-0015-cl-linear-instantiation.md) | CirkaLang で線形型の値が Type の型変数や IO の結果を通じて複製できる | done | critical | FEAT-0212 |
| [BUG-0016](bugs/BUG-0016-opt-stale-known-after-beta.md) | β 簡約の後に既知の構築子の表が古い引数を指す | done | major | FEAT-0220 |
| [BUG-0017](bugs/BUG-0017-cl-invariant-trait-method.md) | トレイトのメソッドを使う不変条件が check で ICE になり、W1231 が誤って出る | done | major | FEAT-0235 |
| [BUG-0018](bugs/BUG-0018-cl-smt-int-wraps.md) | 検証が Int を数学の整数として扱い、ラップする演算で偽の証明が通る | done | critical | FEAT-0235 |
| [BUG-0019](bugs/BUG-0019-cl-meta-table-index.md) | 単一化が作るメタ変数で、メタ変数の位置の表がずれて ICE になる | done | major | FEAT-0235 |
| [BUG-0020](bugs/BUG-0020-lazy-con-captured-thunk.md) | 遅延位置の構築子の正格フィールドに入れた捕捉変数・仮引数が要求前に評価される | done | major | BUG-0014, FEAT-0083 |
| [BUG-0021](bugs/BUG-0021-opt-inline-once-copies.md) | インライン化で、常にインライン化される関数を経由して 1 回だけ呼ばれる関数が指数的に複製される | done | major | FEAT-0241 |
| [BUG-0022](bugs/BUG-0022-cl-case-refines-unsolved-metas.md) | 変数での場合分けが、その変数を引数に持つ未解決のメタ変数を解けなくする | done | major | FEAT-0242 |
| [BUG-0023](bugs/BUG-0023-cl-prim-outside-libraries.md) | 利用者のプログラムに `prim` 宣言を書け、検査のない配列の読み出しでメモリ安全を破れる | done | critical | FEAT-0244 |
| [BUG-0024](bugs/BUG-0024-runtime-tests-descriptor-race.md) | ランタイムの単体テストが記述子番号の再利用で競合し、まれに失敗する | done | minor | FEAT-0242 |
| [BUG-0025](bugs/BUG-0025-runtime-siginfo-size.md) | ランタイムの `siginfo_t` が 8 バイト短く、`sigtimedwait` がスタックを壊す | done | critical | FEAT-0242, FEAT-0047 |
| [BUG-0026](bugs/BUG-0026-cl-instance-solution-with-pending-metas.md) | 型の中の多重定義された演算 (`Tagged (2 + 5) Bool`) でインスタンスの解決が ICE になる | todo | minor | FEAT-0250 |
| [BUG-0027](bugs/BUG-0027-mir-partial-application-over-closure-arity.md) | 残りの引数が 8 個を超える部分適用が C0200 になる (`-O0` の extern の `IO` 関数など) | done | major | FEAT-0251 |
| [BUG-0028](bugs/BUG-0028-native-libm-from-compiler-builtins.md) | ネイティブのプログラムが libm の一部の関数 (`cbrt`・`fma` など) をランタイムの compiler-builtins から取る | done | major | FEAT-0252 |
| [BUG-0029](bugs/BUG-0029-checker-evaluates-array-loops.md) | 検査器が配列を作る繰り返しを評価して指数時間・メモリを使う | done | major | FEAT-0254 |
| [BUG-0030](bugs/BUG-0030-type-uses-later-function-silently.md) | 型が後で宣言する関数を使うと、診断なしに誤りの項になる | done | minor | FEAT-0254 |
| [BUG-0031](bugs/BUG-0031-same-call-translated-differently.md) | 同じ呼び出しが証明義務の中で深さによって別の項に訳され、仮定と目標が一致しない | done | major | FEAT-0254 |
| [BUG-0032](bugs/BUG-0032-float-in-change-not-reported.md) | float-in が正格な束縛を遅延にした変更を報告せず、死んだ組が一意な配列を共有にする | done | major | FEAT-0255 |
| [BUG-0033](bugs/BUG-0033-erased-proof-forged.md) | 消去される位置の証明を `panic` や全域でない関数で作れ、型の変換でメモリ安全を破れる | done | critical | FEAT-0231, FEAT-0260 |
| [BUG-0034](bugs/BUG-0034-io-step-chosen-by-case-allocates.md) | `do` の途中の `if c then act else pure ()` が、選んだ動作を反復ごとにクロージャとして作る | done | major | FEAT-0261 |
| [BUG-0035](bugs/BUG-0035-thunk-used-in-several-alternatives.md) | 複数の選択肢で使う遅延束縛が、使わない反復でもサンクとして確保される | done | major | BUG-0034 |
| [BUG-0036](bugs/BUG-0036-borrowed-sibling-keeps-parent.md) | 読むだけのフィールドの借用が、マッチしたオブジェクトを生かし、別のフィールドの in-place 更新がコピーになる | done | major | BUG-0034 |
| [BUG-0037](bugs/BUG-0037-cascading-instance-error.md) | 引数の型が合わないと、その引数で決まる型の制約が E1254 として重ねて報告される | done | minor | FEAT-0262 |
| [BUG-0038](bugs/BUG-0038-memcheck-without-valgrind.md) | `scripts/memcheck.sh` が実行テストを valgrind の下で走らせていない | in-progress | major | FEAT-0096 |
| [BUG-0039](bugs/BUG-0039-small-function-inlining-exponential.md) | 小さい関数の連鎖 (次の関数を 2 か所から呼ぶ) をインライン化すると 1 回の簡約で指数的に複製し、Core の最適化が終わらない | done | major | FEAT-0097 |
| [BUG-0040](bugs/BUG-0040-z3-counterexample-depends-on-load.md) | 検証の反例が Z3 の実行時間で変わり、負荷が高いと `verify/fail/overflow` の期待出力と食い違う | todo | minor | BUG-0038 |
| [BUG-0041](bugs/BUG-0041-test-case-without-limits.md) | テストハーネスにケースごとの時間とメモリの上限がなく、終わらない 1 ケースがホストのメモリを使い切る | done | major | FEAT-0097 |
| [BUG-0042](bugs/BUG-0042-valgrind-bypasses-size-classes.md) | valgrind の下ではランタイムのアロケータが大きさのクラスを使わず、すべてを malloc で確保する (64 GiB の予約が断られる) | done | minor | FEAT-1001 |
| [BUG-0043](bugs/BUG-0043-region-check-cost.md) | BUG-0042 の修正で解放ごとの領域の判定が重くなり、binary-trees が 4% 遅くなった | done | minor | BUG-0042 |

新しいチケットは `features/_TEMPLATE.md` / `bugs/_TEMPLATE.md` をコピーして作成する。
