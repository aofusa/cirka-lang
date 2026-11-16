# FEAT-0256: 非公開の構成子 (抽象データ型)

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P0
- 依存: FEAT-0250
- 設計: [spec-v0.0.6.md](../../design/cirkalang/spec-v0.0.6.md) §2、ADR-096

## 概要

C のライブラリを安全に包む型 (FEAT-0251 の Vulkan のバッファなど) は、ハンドルや C のメモリへのポインタを持つ。
構成子が公開されていると、利用者が任意のポインタから値を作れ、包んだ関数を通してメモリ安全性を壊せる。
CirkaLang にはエクスポートの一覧もフィールドの可視性もないので、型は公開したまま構成子だけを非公開にする手段を加える。

```
data Buffer (s : Region) (e : Type) = private MkBuffer VkBuffer (Ptr e) Int
data File : LType where
  private MkFile : Int -> File
```

非公開の構成子は、宣言したモジュールの外では式にもパターンにも書けない (E1106)。型・公開の構成子・関数は従来どおり使える。

## 作業内容

- [x] 構文: 短い形と GADT の形で、構成子の前に `private` を書ける (AST の `ConDecl.private`、表示)
- [x] 名前解決: 非公開の構成子を他のモジュールから参照すると E1106 (式・パターン)
- [x] 修飾なしで他のモジュールの非公開の名前を書いたときも、E1101 (未定義) ではなく E1106 (非公開) と報告する
- [x] パターンの構成子が解決できなくても、その中の変数を束縛したものとして扱う (変数の使用について誤りを重ねない)
- [x] 仕様 (spec-v0.0.6.md §2)・ADR-096

## 受け入れ条件

- [x] 他のモジュールから非公開の構成子で値を作る・照合すると E1106、同じモジュールでは使える
      (`cirkalang-resolve` の `private_constructors`、`tests/cirkalang/modules/private_con_fail`)
- [x] 導出 (`deriving`) は非公開の構成子の型にも働く (`tests/cirkalang/packages/abstract`: `Show` と `Eq` を導出した型を他のモジュールから使う)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 構文の表示は `tests/cirkalang/parse/private_con.cirka` (短い形と GADT の形、1 行に複数の構成子)。
- パッケージの版 (`edition`) は v0.0.6 のリリース (FEAT-0253) までは `0.0.5` のまま。

