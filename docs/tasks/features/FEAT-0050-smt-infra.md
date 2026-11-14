# FEAT-0050: SMT 連携基盤 (cirka-smt)

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0005
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

SMT-LIB2 の生成、Z3 プロセス呼び出し、結果キャッシュ、反例の取得。CirkaLang の検証 (v0.0.4) と aCHC の注釈検証が使う。

## 作業内容

- [x] SMT-LIB2 の項・コマンド (`Expr`: 整数・真偽値・演算子・未解釈関数。切り捨て除算 `tdiv` / `trem`)
- [x] Z3 呼び出し (`CIRKA_Z3`, `CIRKA_VERIFY_TIMEOUT`)
- [x] キャッシュ (`_build/<名前>/verify/`。問い合わせの文書ごと記録し、文書が一致したときだけ使う)
- [x] モデル (反例) の取得 (`get-value`)

## 受け入れ条件

- [x] 算術・未解釈関数の簡単な問い合わせのテスト (Z3 がなければスキップ)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 外部プロセスに SMT-LIB を渡す方式にした (ADR-077)。`CIRKA_Z3` が空なら `Solver::from_env` は `None`。
- 最初の利用者は CirkaLang の全域性の尺度 (FEAT-0230)。
