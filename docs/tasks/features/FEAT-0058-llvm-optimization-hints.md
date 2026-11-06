# FEAT-0058: 最適化のヒントを LLVM の属性・メタデータまで運ぶ

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0014, FEAT-0021, FEAT-0022
- 設計: [16-performance.md](../../design/16-performance.md) §3、ADR-047

## 概要

生成する LLVM IR には関数・引数の属性やメタデータがまったく付いていない。型・所有権・Perceus の解析で分かっている事実を
LLVM に渡し、LLVM の別名解析・ループ最適化・ベクトル化を効かせる (16-performance.md §3)。

## 作業内容

- [ ] ~~ヒープ参照の引数・結果に `nonnull` `dereferenceable(8)` `align 8` を付ける~~ → FEAT-0062 (再利用トークンが null になりうる)
- [ ] ~~借用 (borrowed) 引数に `readonly` `nocapture` を付ける~~ → FEAT-0062 (借用引数も dup して保持されうる)
- [ ] ~~一意な値の引数に `noalias` を付ける~~ → FEAT-0062 (MIR に一意性の情報がない)。確保の結果 (`cirka_alloc`) には `noalias nonnull` を付けた
- [x] ヘッダ (RC・タグ) とフィールドの load/store に `!tbaa` を付け、別名にならないことを伝える
- [x] panic に `cold` (分岐の重みは既存)。タグの load への `!range` は計測で遅くなったので外した
- [x] panic=abort の間はすべての関数に `nounwind` を付ける
- [x] ベンチで前後を計測し bench-report に記録

## 受け入れ条件

- [x] 属性・メタデータを確かめる LLVM IR のテスト (`cirka-codegen-llvm` の `emits_optimization_hints`)
- [x] 全実行テストがリーク 0 のまま通る (`--runtime=debug` を含む)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ
- 計測 (bench-report「LLVM の属性・メタデータ」): 効果は ±5% で小さい。今の差の主因は確保の呼び出し・箱・サンクで、別名の情報が効く場所がまだ少ない。
- タグの load の `!range` は mandelbrot を 10% 遅くした (属性ごとに外して二分探索で特定)。ほかで効かなかったので外した。
- TBAA の妥当性: ヘッダ (先頭 8 バイト) とフィールド (8 バイト目以降) は生きているオブジェクトの中で重ならない。解放後に別のサイズで再利用された
  メモリでは重なりうるが、解放はランタイムの呼び出しを通るので、その前後で load/store が入れ替わることはない。配列の要素・バイト列は従来どおり別名の情報なし。
- 引数の属性 (`nonnull`・`noalias`・`readonly`) は MIR の事実が足りないので FEAT-0062 に分けた。
