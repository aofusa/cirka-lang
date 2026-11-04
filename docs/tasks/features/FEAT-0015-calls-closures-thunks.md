# FEAT-0015: 末尾呼び出し・クロージャ・サンク

- 状態: todo
- マイルストーン: M1 バックエンド基盤
- 優先度: P0
- 依存: FEAT-0014
- 設計: [05-runtime.md](../../design/05-runtime.md), [07-backend.md](../../design/07-backend.md)

## 概要

Haskell の実行に必要な関数呼び出しの仕組みを MIR・ランタイム・コード生成で実装する。

## 作業内容

- [ ] `tailcc` + `musttail` による末尾呼び出し
- [ ] `FUN`/`PAP` と eval/apply (`apply_n`)
- [ ] `THUNK`/`BLACKHOLE`/`IND` と `force` (`<<loop>>` 検出)
- [ ] 再帰クロージャグループの自己参照を RC 対象外にする

## 受け入れ条件

- [ ] 10^8 回の相互再帰ループがスタックを消費せずに動く
- [ ] 部分適用・過剰適用・サンク共有のテストがインタプリタとネイティブで一致
- [ ] リーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

