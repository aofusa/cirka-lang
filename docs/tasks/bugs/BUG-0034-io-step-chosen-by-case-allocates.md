# BUG-0034: `do` の途中の `if c then act else pure ()` が、選んだ動作を反復ごとにクロージャとして作る

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0261 (`bench/effects/direct.cirka`)
- 関連: FEAT-0261, BUG-0011, FEAT-0074

## 現象

`bench/effects/direct.cirka` (効果のハンドラを名前で呼ぶ版) が、効果を使う `main.cirka` の 7 倍遅い
(ホストで 1 億回: 5.9 s と 0.81 s。Rust は 0.8 s)。結果は正しい。

`-O2` の Core では、`do` の途中の `if i % every == 0 then outD h else pure ()` が選んだ動作 (`IO` にキャストした λ) を
`let!` で束縛してから状態 `w` に適用しており、反復ごとにクロージャを確保して間接に呼んでいた:

```
(let! (t (IO Unit) (case (prim eq u 0) (b Int#) (IO Unit)
    (0 (cast (lam ((a Unit#)) ... MkUnit) (IO Unit)))
    (_ (cast (lam ((a Unit#)) (let! (w1 Unit# (extern cirka_write_i64 x)) ...)) (IO Unit)))))
  (let! (x Unit ((cast t (-> Unit# Unit)) w))
    ...))
```

## 再現手順

```
module Main

loop : Int -> Int -> IO Int
loop i acc = if i == 0 then pure acc else do
  if i % 25000000 == 0 then printInt i else pure ()
  loop (i - 1) (acc + i)

main : IO ()
main = do
  r <- loop 100000000 0
  printInt r
```

```sh
cirka build -O2 --dump=core-opt -o w when.cirka   # Main.loop$io$w に (cast (lam ((a Unit#)) が残る
./w                                               # ホストで 4.59 s
```

## 期待される動作

選んだ動作は `case` の選択肢の中で `w` に適用され、確保も間接呼び出しもない (効果の版の `-O2` はそうなっていた)。

## 原因

`IO` の `>>=` をインライン化すると、非アトムの `m` は `let! t = m` に束縛され、残りが最初に評価するのが `t w` になる。
`let!` の右辺が λ なら遅延束縛として 1 回の使用にインライン化する規則 (BUG-0011) はあったが、右辺が選択肢ごとに λ を返す `case` のときは
束縛がそのまま残り、`case` の結果のクロージャを作ってから呼んでいた。効果の版は `primEffRun` の展開で `(case …) w` の形になり、
既存の規則 `(case s of p → e) a ⇒ case s of p → e a` で適用が選択肢に入っていた。

## 修正

簡約器 (`opt.rs` の `let_strict`): `let! t = e in let! x = (cast t τ) a… in k` で、`t` がその呼び出しの 1 回だけ (λ の外) で使われるとき、
`e` をその位置に代入する。`t` の呼び出しは残りが最初に評価するものなので (引数はアトム)、`e` を評価する時点は変わらない。
代入の後、キャストは `case` の選択肢に入り、既存の規則で適用が選択肢に入って λ がβ簡約される。

- [x] 回帰テストを追加 (`cirka-core` の `opt_applies_an_action_chosen_by_a_case_in_its_branches`)
- [x] 修正

## メモ

- 計測 (ホスト、`cirka build -O2`): 再現手順のループ 4.59 s → 0.22 s。`bench/effects/direct.cirka` (1 億回) は 5.9 s → 0.66 s。
- `bench/effects` (Docker、hyperfine 15 回): 名前で呼ぶ版 668.9 ± 15.0 ms、効果の版 549.6 ± 12.4 ms、Rust 674.9 ± 90.6 ms
  (ホストの別の負荷で Rust の揺らぎが大きい)。命令数 (callgrind、1000 万回) は 330.2 M / 260.2 M / 250.3 M。
  名前で呼ぶ版に残る 1 反復あたり 7 命令は、結合点で `()` (静的なオブジェクト) を `drop` する分で、FEAT-0089 に分けた。
- 全テスト (ホストの achc・bench・cirkalang・core・driver・mir) が通り、期待出力の変更はなかった。
- Docker の CI で、`linalg.cirka` の配列のコピーが 1 回増えた (9 → 10)。動作が腕に入った後、Perceus が読むだけのフィールド (`Order`) の借用で
  行列を生かし、配列のフィールドの in-place 更新がコピーになった。Perceus の問題として BUG-0036 で直した (9 回に戻る)。
- FEAT-0088 (`linalg.cirka` の `gemm` の IO の動作がクロージャになる) も同じ形の可能性があるので、FEAT-0088 のメモに書いた。
- aCHC の同じ形 (`when (i `mod` n == 0) (print i)`) は、`print i` が 3 つの選択肢で使う遅延束縛として外に残る別の原因で、
  この修正では変わらない (2.5 s)。BUG-0035 に分けた。
