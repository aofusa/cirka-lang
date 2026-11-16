# FEAT-0255: 大きな extern のモジュールのコンパイル時間

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P1
- 依存: FEAT-0250
- 設計: [spec-v0.0.6.md](../../design/cirkalang/spec-v0.0.6.md) §1.7・§1.10、[01-architecture.md](../../design/01-architecture.md) §5、ADR-095、[16-performance.md](../../design/16-performance.md)

## 概要

`cirka bindgen` が `vulkan_core.h` 全体から書いたモジュール (構造体 981・共用体 10・列挙 265・型 1189・定数 1403・関数 623 の宣言、約 2 万行。関数ポインタの型 `PFN_*` は型に数える) は
`cirka check` を通るが、約 50 秒かかる (FEAT-0250 で計測。Docker 内、リリースビルド)。
extern ブロックの構造体・共用体・列挙はそれぞれ `Eq`・`Show`・`Storable` などの impl を生成するので、このモジュールには数千の impl がある。
Vulkan のサンプル (FEAT-0251) がこのモジュールを使えるよう、コンパイル時間を数秒にする。

## 作業内容

- [x] 計測: どの段階 (構文解析・展開・名前解決・エラボレーション・Core 以降) に何秒かかるかを測る (`--time-passes`)
- [x] インスタンス探索 (`elab/traits.rs` の `search`) で、トレイトの impl の一覧を探索のたびに複製せず、
      頭の型構成子で引ける索引にする (探索の回数 × impl の数にならないようにする)
- [x] エラボレーションの環境 (`Vec<Val>`) の複製を減らす (`Ev::inst` / `apply` で多い)
- [x] 残りの段階で数に比例しない処理があれば直す (Core Lint の索引・データ型の種の記憶、単純化の安定した束縛の省略、最初の単純化の後の dce)
- [x] extern ブロックから展開した項目は、使うものだけをエラボレーション・変換する (`Origin`、`elab/used.rs`、`--extern-items=all`)
- [x] `cirka check` は Core を最適化しない (01-architecture.md の「コード生成しない」どおり。`-O` を明示すれば最適化)
- [x] 仕様 (spec-v0.0.6.md §1.10)・01-architecture.md (`--time-passes`・`--extern-items`)・ADR-095 を書く
- [x] `bindgen_system_headers` で vulkan_core.h 全体の CirkaLang 出力をコンパイルし時間を表示する
- [x] 大きな extern のモジュールのコンパイル時間を測るベンチ (生成した宣言、または `vulkan_core.h` の出力) を加える
      → `bench/` は実行時の性能を Rust / GHC と比べる場なので、コンパイル時間は `bindgen_system_headers` が測る
      (vulkan_core.h 全体の出力を、構造体・列挙・定数・関数を使う Main とともに `check` と同じ設定と `-O2` でコンパイルして時間を表示し、30 秒を超えたら失敗)

## 受け入れ条件

- [x] `vulkan_core.h` 全体の出力の `cirka check` が 5 秒以内 (Docker 内、リリースビルド): 1.9〜2.7 s
- [x] 計測の結果 (前後の時間と段階ごとの内訳) をチケットに記録する
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- FEAT-0250 での観察 (gdb で 2 秒ごとに 24 回スタックを採取): 約 4 割がインスタンス探索 (`Elab::search` → `try_impl` → `match_val`。
  `solve_pending` と impl の本体の検査から呼ばれる) で、`self.prog.impls.get(&tr).cloned()` による一覧の複製と、
  `Ev::apply` / `Ev::inst` の `Vec<Val>` の複製が目立つ。Core の Lint にも少し時間がかかる。

### 結果 (2026-11-16)

vulkan_core.h 全体の出力 (bindgen で生成、20391 行) を import する Main の `cirka check` (Docker 内、リリースビルド、3 回の中央値):

| Main | 前 (FEAT-0250) | 後 |
|------|----------------|----|
| 何も使わない (`main = pure ()`) | 51.4 s / 666 MB | 1.99 s / 446 MB |
| 構造体 (106 フィールドの `VkPhysicalDeviceLimits` を含む `VkPhysicalDeviceProperties`)・列挙・定数・関数を使う | — | 2.69 s |

`--time-passes` による内訳 (後、何も使わない Main / 使う Main): 構文解析と名前解決 1.19 / 1.12 s、エラボレーション 0.39 / 0.78 s、
全域性と洗練型 0.01 / 0.02 s、Core への変換 0.22 / 0.30 s、Core の dce 0.03 s、Core → MIR・Perceus・検証器・LLVM IR 0.01 / 0.27 s。
名前解決の内訳 (ホスト): extern の展開 0.18 s・deriving 0.07 s・Vk の名前解決 0.66 s (53481 の宣言)。

効いた変更:

- **使う項目だけを検査・変換する** (spec-v0.0.6.md §1.10、ADR-095): 何も使わない Main で 11.1 s → 1.9 s (ホスト。エラボレーションと変換の大半が、使わない構造体のコードだった)。
  51.4 s → 11.1 s は以下の変更による。
- **インスタンス探索の索引** (トレイトと頭の型構成子で impl を引く) と、一貫性の検査で頭を一度だけ評価する: 探索が impl の数に比例しなくなった。
- **環境の複製を減らす** (`Ev::eval` が環境のスライスを受ける、`apply_all`、依存しない引数を評価しない)。
- **Core Lint の索引** (大域・データ型・構成子の表を 1 度だけ作り、引数のないデータ型の種を記憶する) と、フロントエンドが Lint した Core をドライバがもう一度 Lint しない。
- **Core の単純化**: `live_globals` を線形にし (以前は `p.bind` で O(n²))、最初の単純化の後に dce し、前のラウンドで変わらず参照先の候補も変わらない束縛と死んだ束縛を飛ばす。
  構造体を使う Main の `-O2` の Core 最適化は 6.7 s → 2.7 s (ホスト)。残りは深く入れ子の `Main.main` の単純化が収束しないためで、FEAT-0087 に分けた。
- **`check` は Core を最適化しない** (両 CLI。`-O` を明示すれば最適化する)。

途中で試して取りやめたもの: 単純化のラウンドごとの dce (特殊化の名前 `$sN` が再利用されて辞書のフィールド `$fN` と衝突し、ICE になった)。

テスト: `cirkalang_extern_items_on_demand` (使わない項目が Core にないこと、`--extern-items=all` ではあること)、
`check_does_not_optimize_unless_asked` / `time_passes_is_a_driver_flag` (CLI)、`bindgen_system_headers` の Vulkan の Main (Docker 内で check 2.50 s・`-O2` 2.15 s)。
