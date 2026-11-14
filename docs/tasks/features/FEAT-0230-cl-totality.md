# FEAT-0230: 全域性・終了性の検査

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P0
- 依存: FEAT-0223
- 設計: [13-verification.md](../../design/13-verification.md)、[spec-v0.0.4.md](../../design/cirkalang/spec-v0.0.4.md) §1

## 概要

網羅性・終了性 (構造的再帰 + 尺度) を検査し、`total` / `covering` / `partial` を区別する。型・証明に使う関数は total 必須。

## 作業内容

- [x] 網羅性 (case tree): `covering` は従来の E1230、`partial` は欠けた場合を `panic` で埋める
- [x] 構造的再帰の判定 (size-change): 相互再帰・引数の入れ替え、構成子の組み直しは等しいとみなす、正でないデータ型は全域でない
- [x] 尺度注釈 `decreasing e` (Z3 で証明義務を決める。反例を表示)。`Int` / `Nat` の引数は自動で尺度として試す
- [x] total 指定の伝播: 全域性は推論し、`total` の関数と型の中の関数 (E0505) から辿れる関数を調べる。トレイトのメソッドは既知のインスタンスまで辿る

## 受け入れ条件

- [x] ackermann 等が pass、非停止の例が fail
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装: `crates/cirkalang-check` (walk.rs で本体を NbE の値で辿って呼び出しの大きさの関係と経路の条件を集め、lib.rs で SCC ごとに size-change の閉包を作る。measure.rs で尺度の証明義務を SMT に訳す)。構文は `total` / `covering` / `partial` (シグネチャの前) と `decreasing e` (節の右辺の後)。
- 共通基盤の SMT 連携 (FEAT-0050) を同時に実装した。
- テスト: `tests/cirkalang/run/v004/totality`、`fail/total/*` (E0502 / E0504 / E0505 / E1230、既定のメソッドが呼び合うループ)、`verify/{pass,fail}` (Z3 が必要。Docker で実行)、`parse/totality`、`cirkalang-check` の単体テスト。
- 制限 (spec §1.3): 尺度は直接の再帰だけ、`Int` は数学の整数として扱う、`extern` は全域とみなす。
