# FEAT-0120: LinearTypes (多重度)

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P0
- 依存: FEAT-0110
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md), [03-type-system.md](../../design/03-type-system.md)

## 概要

GHC LinearTypes 互換の多重度と `%0`、let の多重度推論、Core Lint の多重度検査。

## 作業内容

- [x] 多重度の構文と型 (`%1`/`%0`/`%m`/`%Many`/`⊸`、型 `#FUN m a b`)
- [x] 使用量の検査 (`achc-typeck/src/linear.rs`)
- [x] let の多重度推論
- [x] `Prelude.Linear` 最小サブセット (+ `Unsafe.Linear`)

## 受け入れ条件

- [x] linear-base の README 程度の例が通る (`tests/achc/run/h3/linear_{basic,prelude}.hs`)
- [x] E0302/E0303 のエラーテスト (`tests/achc/typeck/fail/linear.hs`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 方式は ADR-037 / 03-type-system.md §2.4。多重度は型として HM に乗せ、使用量は推論後の別パスで検査する。
- 多重度は脱糖で消去する。Core Lint の多重度検査は、Core に線形情報が入る一意性 (FEAT-0121) と CirkaLang の側で行う
  (aCHC の前期では Core に多重度 1 の関数が現れない)。
- `linear_basic.hs` は GHC 9.4 でも通る範囲 (GHC との差分テストの対象)。`let` の多重度推論と `Prelude.Linear` を使う
  `linear_prelude.hs` は `-- ghc: skip` (実行テストが GHC との比較を飛ばす印。`tests/achc.rs`)。
- 未対応: `let %1 x = ...` の注釈、`UnicodeSyntax` の `⊸` 以外の記号、線形の `IO` (linear-base の `System.IO.Linear`)。

