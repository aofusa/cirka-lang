# FEAT-0150: 依存量化 (foreach / 必須型引数) とシングルトン

- 状態: todo
- トラック: aCHC
- マイルストーン: H6 依存型と検証 (CirkaLang v0.0.4 の後)
- 優先度: P1
- 依存: FEAT-0144
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

Dependent Haskell の方向に沿う最小限: `forall a ->`、`foreach (n :: Nat) ->` をシングルトン受け渡しへ elaborate。CirkaLang v0.0.4 の評価を反映する。

## 作業内容

- [ ] RequiredTypeArguments
- [ ] `foreach` とシングルトンへの elaboration
- [ ] `Cirka.Singletons` と `deriving Sing`
- [ ] foreach 引数の多重度

## 受け入れ条件

- [ ] `replicate :: foreach n -> a -> Vect n a` の例が動き、消去される部分が実行時に残らない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

