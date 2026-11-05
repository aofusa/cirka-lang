# FEAT-0132: aCHC の評価レポート

- 状態: in-progress
- トラック: aCHC
- マイルストーン: H4 FFI 拡張と評価
- 優先度: P0
- 依存: FEAT-0117, FEAT-0125, FEAT-0043
- 設計: [00-overview.md](../../design/00-overview.md)

## 概要

aCHC で検証した事項 (00-overview §7) をまとめ、CirkaLang v0.1.0 の設計に入力する。

## 作業内容

- [x] GHC 比の性能・メモリ
- [x] 遅延評価と RC の相性・循環の頻度
- [x] 所有権拡張の書き味
- [x] Haskell から捨てるべきもの

## 受け入れ条件

- [ ] `docs/design/achc-evaluation.md` がレビュー済み (草稿を作成済み。利用者のレビュー待ち)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 草稿: [achc-evaluation.md](../../design/achc-evaluation.md) (2026-11-05)。§8 の 2・3・4・7 を、bench-report (FEAT-0043/0056)・
  compat-report (FEAT-0119/0126)・H3 の各チケットのメモと、本チケットで取り直した `CIRKA_STATS` の確保回数からまとめた。
- テスト: `tests/workspace.rs` の `doc_links_resolve` (docs/ と AGENTS.md の相対リンクがすべて存在するファイルを指す)。
  レポートは他の設計書・チケットを根拠として参照するため。
- FEAT-0130 (`extern "C"` ブロック) は FEAT-0047 待ちで未着手。FFI の評価は FEAT-0116 の範囲で行った。
- レビューで指摘があれば反映し、レビュー済みになった時点で done にする。

