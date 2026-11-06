# FEAT-0058: 最適化のヒントを LLVM の属性・メタデータまで運ぶ

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0014, FEAT-0021, FEAT-0022
- 設計: [16-performance.md](../../design/16-performance.md) §3、ADR-047

## 概要

生成する LLVM IR には関数・引数の属性やメタデータがまったく付いていない。型・所有権・Perceus の解析で分かっている事実を
LLVM に渡し、LLVM の別名解析・ループ最適化・ベクトル化を効かせる (16-performance.md §3)。

## 作業内容

- [ ] ヒープ参照の引数・結果に `nonnull` `dereferenceable(8)` `align 8` を付ける
- [ ] 借用 (borrowed) 引数に `readonly` `nocapture` を付ける
- [ ] 一意な値 (再利用・破壊的更新の対象) の引数に `noalias` を付ける (MIR 検証器で整合を確かめる)
- [ ] ヘッダ (RC・タグ) とフィールドの load/store に `!tbaa` を付け、別名にならないことを伝える
- [ ] `switch` のタグの load に `!range`、panic・RC の遅い経路に `cold` と分岐の重みを付ける
- [ ] panic=abort の間はすべての関数に `nounwind` を付ける
- [ ] ベンチ (n-body・spectral-norm・binary-trees・mandelbrot) で前後を計測し bench-report に記録

## 受け入れ条件

- [ ] 属性・メタデータを確かめる LLVM IR のスナップショットテスト
- [ ] 全実行テストがリーク 0 のまま通る (`--runtime=debug` を含む)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
