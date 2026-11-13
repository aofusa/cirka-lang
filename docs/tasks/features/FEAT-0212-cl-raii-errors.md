# FEAT-0212: RAII (Drop/Release) とエラー処理

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0210
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)、[spec-v0.0.2.md](../../design/cirkalang/spec-v0.0.2.md) §5〜6

## 概要

`Drop`/`Release` の自動挿入、`Result` と `?` 相当のエラー伝播、`panic` (abort。0040 後は unwind)。

## 作業内容

- [x] Drop / Release (トレイト、メモリだけの型の組み込みの実装、終了処理の挿入パス `raii.rs`、導出、コンテナの実装、W0308)
- [x] 結果が線形なアクション `IO1` と、`IO` / `IO1` の `do` の束縛の選択
- [x] Result とエラー伝播構文 (`Result`、`Functor` / `Monad`、`?`)
- [x] panic (`unwrap` `expect` `expectJust`。abort のまま。unwind は FEAT-0040)
- [x] 設計中に見つけた線形性の穴の修正 (BUG-0015)

## 受け入れ条件

- [x] 解放順序の run テスト (`tests/cirkalang/run/v002/raii_release`、`raii_drop`、`raii_io_value`)
- [x] エラー伝播の run テスト (`tests/cirkalang/run/v002/result_try`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 仕様は spec-v0.0.2.md §5 (所有権と資源) と §6 (エラー処理)、設計判断は ADR-059〜063。
- 終了処理は、型検査とメタ変数の解決が終わった本体に対する独立したパスで挿入する。アクションの中ではパスの終わりに後に束縛したものから、
  純粋な式ではパスの先頭に、関数のパラメータは項数を保って入れる。挿入後の TT は Core Lint を通る。
- `?` は `IO` / `IO1` の `do` の文末にだけ書ける。`Result` の値にもアクションにも使え、誤りの型は変換しない。
  `IO1 (Result e Res)` の早期リターンでも、それまでに開いた資源は解放される (手で確認)。
- 失敗のテスト: `fail/own/` の `release_pure` (E0307)、`fin_no_impl` (E0302 と導出の案内)、`fail/type/` の `try_not_result` (E1260)、
  `drop_shareable` (E1258)。リークの警告 W0308 は `cirkalang-elab` の単体テスト `unused_holders_of_finalized_values_warn`。
- 制限: 保持者を受け取った関数がそれを捨てる場合のリークは検出しない。`panic` では終了処理を実行しない (FEAT-0040 の後に unwind で行う)。
