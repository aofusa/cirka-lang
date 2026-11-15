# FEAT-0255: 大きな extern のモジュールのコンパイル時間

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P1
- 依存: FEAT-0250
- 設計: [spec-v0.0.6.md](../../design/cirkalang/spec-v0.0.6.md) §1.7、[16-performance.md](../../design/16-performance.md)

## 概要

`cirka bindgen` が `vulkan_core.h` 全体から書いたモジュール (構造体 981・共用体 10・列挙 265・型 1189・定数 1403・関数 1889 の宣言、約 2 万行) は
`cirka check` を通るが、約 50 秒かかる (FEAT-0250 で計測。Docker 内、リリースビルド)。
extern ブロックの構造体・共用体・列挙はそれぞれ `Eq`・`Show`・`Storable` などの impl を生成するので、このモジュールには数千の impl がある。
Vulkan のサンプル (FEAT-0251) がこのモジュールを使えるよう、コンパイル時間を数秒にする。

## 作業内容

- [ ] 計測: どの段階 (構文解析・展開・名前解決・エラボレーション・Core 以降) に何秒かかるかを測る
- [ ] インスタンス探索 (`elab/traits.rs` の `search`) で、トレイトの impl の一覧を探索のたびに複製せず、
      頭の型構成子で引ける索引にする (探索の回数 × impl の数にならないようにする)
- [ ] エラボレーションの環境 (`Vec<Val>`) の複製を減らす (`Ev::inst` / `apply` で多い)
- [ ] 残りの段階で数に比例しない処理があれば直す
- [ ] 大きな extern のモジュールのコンパイル時間を測るベンチ (生成した宣言、または `vulkan_core.h` の出力) を加える

## 受け入れ条件

- [ ] `vulkan_core.h` 全体の出力の `cirka check` が 5 秒以内 (Docker 内、リリースビルド)
- [ ] 計測の結果 (前後の時間と段階ごとの内訳) をチケットに記録する
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- FEAT-0250 での観察 (gdb で 2 秒ごとに 24 回スタックを採取): 約 4 割がインスタンス探索 (`Elab::search` → `try_impl` → `match_val`。
  `solve_pending` と impl の本体の検査から呼ばれる) で、`self.prog.impls.get(&tr).cloned()` による一覧の複製と、
  `Ev::apply` / `Ev::inst` の `Vec<Val>` の複製が目立つ。Core の Lint にも少し時間がかかる。
