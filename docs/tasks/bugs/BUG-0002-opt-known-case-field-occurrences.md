# BUG-0002: Core 簡約器の case-of-known でフィールドの束縛が消える

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0205 (`tests/cirkalang/run/v001/adt.cirka` の `fst (snd (1, (2, 3)))`)
- 関連: FEAT-0033

## 現象

`-O1` 以上で Core Lint が ICE を報告する: `variable `y` (#23) is not in scope`。
`let! t1 = MkPair x y` の後で `fst t1` をインライン化して case-of-known-constructor を行うと、`let! y = ...` が削除される。

## 再現手順

```sh
cirka run tests/core/run/opt_known_fields.core
```

## 期待される動作

`1` を出力する (`-O0` と同じ)。

## 原因

`known_scrut` が、選択した選択肢のフィールド束縛子をコンストラクタの引数 (アトム) で置き換えるときに、
`transfer_occ` で引数の出現回数から 1 を引いていた。`transfer_occ` は「束縛 `let x = a` を消して `x` を `a` で置き換える」
場合のためのもので、束縛元の出現 1 回が消えることを前提にしている。case-of-known ではコンストラクタは束縛されたまま残るので、
引数の出現は減らない。出現回数が 0 と誤って数えられた `y` の `let!` が、不要束縛として削除されていた。

## 修正

- [x] 回帰テストを追加 (`tests/core/run/opt_known_fields.core`)
- [x] 修正 (`transfer_occ_by` で消える出現の数を指定し、フィールドでは 0 にする)

## メモ
