# FEAT-0015: 末尾呼び出し・クロージャ・サンク

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M1 バックエンド基盤
- 優先度: P0
- 依存: FEAT-0014
- 設計: [05-runtime.md](../../design/05-runtime.md), [07-backend.md](../../design/07-backend.md)

## 概要

関数呼び出しの仕組み。サンクは aCHC の遅延評価と CirkaLang の `Lazy a` の両方で使う。

## 作業内容

- [ ] `tailcc` + `musttail`
- [ ] `FUN`/`PAP` と eval/apply
- [ ] `THUNK`/`BLACKHOLE`/`IND` と `force`
- [ ] 再帰クロージャグループの自己参照を RC 対象外に

## 受け入れ条件

- [ ] 10^8 回の相互再帰ループがスタックを消費しない
- [ ] 部分適用・過剰適用・サンク共有がインタプリタとネイティブで一致
- [ ] リーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

