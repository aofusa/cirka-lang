# FEAT-0050: SMT 連携基盤 (cirka-smt)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0005
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

SMT-LIB2 の生成、Z3 プロセス呼び出し、結果キャッシュ、反例の取得。CirkaLang の検証 (v0.0.4) と aCHC の注釈検証が使う。

## 作業内容

- [ ] SMT-LIB2 の項・コマンド
- [ ] Z3 呼び出し (`CIRKA_Z3`, `CIRKA_VERIFY_TIMEOUT`)
- [ ] キャッシュ (`_build/verify/`)
- [ ] モデル (反例) の取得

## 受け入れ条件

- [ ] 算術・未解釈関数の簡単な問い合わせのテスト (Z3 がなければスキップ)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

