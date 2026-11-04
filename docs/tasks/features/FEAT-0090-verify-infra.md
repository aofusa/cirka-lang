# FEAT-0090: 検証基盤 (cirka-verify・SMT 連携・achc verify)

- 状態: todo
- マイルストーン: M9 検証機能 (別軸・優先度低)
- 優先度: P1
- 依存: FEAT-0033, FEAT-0037
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

検証パスの土台。Core 上の検証条件 (VC) を SMT-LIB2 で出力し Z3 で解く。`achc verify` と `--verify`、結果キャッシュ、仮定の一覧。

## 作業内容

- [ ] `cirka-verify` クレート
- [ ] SMT-LIB2 生成と Z3 プロセス呼び出し (`CIRKA_Z3`, `CIRKA_VERIFY_TIMEOUT`)
- [ ] 結果キャッシュ (`_build/verify/`)
- [ ] `achc verify [--report]` と `achc.toml` の `[verify]`
- [ ] `tests/verify` ハーネス (Z3 がなければスキップ)
- [ ] 反例の表示 (E0501)

## 受け入れ条件

- [ ] 検証を有効にしないビルドの時間・生成コードが変わらないことを確認
- [ ] 単純な VC (算術) の pass/fail テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

