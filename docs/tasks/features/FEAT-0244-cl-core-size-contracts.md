# FEAT-0244: core の配列・文字列の関数に大きさの契約を付ける

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P2
- 依存: FEAT-0235
- 設計: [spec-v0.0.4.md](../../design/cirkalang/spec-v0.0.4.md) §3, §5

## 概要

v0.0.4 の評価 (FEAT-0235) で、実行時に作った配列の大きさが検証から見えないため、`requires A.size xs == A.size ys` のような契約を
呼ぶ側で `if` で確かめ直す必要があった。`Data.Array.generate` / `new` / `fromList` / `map` / `thaw` / `freeze`、`Data.String` の `slice` などの結果に
大きさの `ensures` を付け、利用者のコードで境界の証明が通りやすくする。

## 作業内容

- [ ] 結果の大きさの契約 (`ensures \r => size r == n` など。`n < 0` のときの扱いを決める)
- [ ] 線形な値 (`Unique (Array a)`) の部分集合型、またはプリミティブの仕様を仮定として与える仕組み (信頼する仕様。`unsafe` と同じくライブラリに閉じる)
- [ ] 検証済みサンプル (`sample_array.cirka`) から呼ぶ側の `if` を外せることを確かめる

## 受け入れ条件

- [ ] `A.generate n f` の結果に対する添字 `0 <= i < n` の `A.index` が証明される
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- v0.0.4 では `LType` の値の部分集合型を扱わない (spec-v0.0.4.md §3.5) ので、一意な配列を作る関数には直接書けない。
