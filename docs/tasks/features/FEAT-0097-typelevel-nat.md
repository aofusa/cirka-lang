# FEAT-0097: 軽量依存型: 型レベル自然数と Cirka.Nat

- 状態: todo
- マイルストーン: M9 検証機能 (別軸・優先度低)
- 優先度: P2
- 依存: FEAT-0075
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

DataKinds/GADTs/型族の上に、型レベル自然数の算術ソルバと `SNat`/`Fin`/`Vect`/`SArray` を提供する。

## 作業内容

- [ ] 型レベル Nat リテラルと +, *, - (GHC.TypeLits 互換)
- [ ] 型検査器に組み込む算術正規化ソルバ
- [ ] `Cirka.Nat` (SNat, Fin, Vect, サイズ付き一意配列 SArray)

## 受け入れ条件

- [ ] `append :: Vect n a -> Vect m a -> Vect (n + m) a` が型検査を通る
- [ ] 範囲外の `Fin` が型エラー
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

