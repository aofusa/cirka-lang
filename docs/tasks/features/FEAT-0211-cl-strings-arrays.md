# FEAT-0211: 文字列と配列

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0210, FEAT-0022
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)、[spec-v0.0.2.md](../../design/cirkalang/spec-v0.0.2.md) §5.6・§7、ADR-064〜068

## 概要

UTF-8 の所有文字列 `String` と借用 `Str`、一意配列 `Array` / 可変長 `Vec` の in-place 操作。

## 作業内容

- [x] String / Str: `Data.String` (ビュー `Str`、分割・検索・変換、`Builder`)。低水準の操作は `private prim` (ランタイムを増やさず `support.core` に書く)
- [x] Array / Vec (Unique で in-place): `Data.Array`、`Data.Vec`、箱なしの `Data.Array.Unboxed` (トレイト `Unbox`)
- [x] 境界検査 (範囲外はパニック。証明による除去は FEAT-0234)
- [x] 付随する言語機能: `ω` のフィールドと `Ur` (§5.6)、`import` の別名と名前の一覧 (§7.1)、`panic` の結果の線形化
- [x] 付随する修正: 資源を持たない判定の遅延 (リテラルの型が後で決まる場合)、メタ変数の刈り込み、構成子のパターンによる型の決定
- [x] 性能: 共通基盤の最適化を FEAT-0079 として分けた

## 受け入れ条件

- [x] 文字列処理・配列ソートのベンチを記録
- [x] 書き込みで確保が発生しない (`array_unique`: 配列の複製 0 回、`uarray_noalloc`: 確保 3 回以下)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

ベンチ (最良値、100 万要素 / 行):

| ベンチ | CirkaLang | Rust | 備考 |
|---|---:|---:|---|
| `bench/array-sort` (UArray のクイックソート) | 0.09 s | 0.08 s | ループ内の確保なし |
| `bench/string-split` (行・単語の分割と `Builder` での出力) | 0.30 s | 0.19 s | 確保 2 M (開始時 24 M、0.895 s)。残りは FEAT-0080 |

テスト: `run/v002/{array_basic,array_bounds,array_unique,uarray,uarray_noalloc,strings,vec}`、`parse/imports`、`modules/{alias_fail,import_list_fail}`、`fail/own/ur_linear`、elab・resolve の単体テスト。

