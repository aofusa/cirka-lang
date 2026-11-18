# FEAT-0263: CirkaLang v0.0.7 リリース

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.7 効果・能力・セッション型
- 優先度: P0
- 依存: FEAT-0261
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

効果・能力の評価とリリース。

## 作業内容

- [ ] CHANGELOG
- [ ] タグ `cirkalang-v0.0.7`

## 受け入れ条件

- [ ] 評価メモの記録
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- **非互換 (CHANGELOG に書く)**: 外の資源に名前で触れる std の操作と時刻・乱数が、最初の引数に能力 `Cap k` を取る (FEAT-0260、spec-v0.0.7.md §1.3)。
  v0.0.6 のプログラムは `main` で能力を受け取り、操作に渡すよう書き換える。リポジトリでは std の操作を使うテスト 11 本
  (`fail/net/socket_*`、`run/v002/std_env`・`std_fs`・`tool_wc`、`run/v003/borrow_file`・`do_patterns`、`run/v005/http_echo`・`net_tcp`・`net_udp`・`os_tool`)
  とベンチ 2 本 (`cli-grep`、`http-server`) を書き換えた。
- `main` の型の誤り (E1200) の文言が変わった (能力を受け取った後に `IO ()` であること)。

