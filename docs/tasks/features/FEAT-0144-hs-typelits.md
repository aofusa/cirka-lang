# FEAT-0144: 型レベル Nat / Symbol (GHC.TypeLits 互換)

- 状態: todo
- トラック: aCHC
- マイルストーン: H5 高度な型システム (CirkaLang v0.0.4 の後)
- 優先度: P1
- 依存: FEAT-0142, FEAT-0141
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

型レベルの算術・文字列、組み込みの算術正規化ソルバ、KnownNat / KnownSymbol。

## 作業内容

- [ ] 型レベルリテラルと演算
- [ ] 算術正規化ソルバ
- [ ] KnownNat / KnownSymbol

## 受け入れ条件

- [ ] `Vect n a` の append などが型検査を通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

