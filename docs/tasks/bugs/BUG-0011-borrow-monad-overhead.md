# BUG-0011: Borrow モナドのループが線形な受け渡しより約 130 倍遅い

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0043 (`bench/n-body/own.chs` の借用版)
- 関連: FEAT-0124, FEAT-0123

## 現象

n-body の 1 ステップ (5 体、約 100 回の読み書き) を `borrowMut` と `Borrow` モナド
(`U.index (asRef m)` / `U.writeAt m`) で書くと、同じ処理を `U.read` / `U.write` で配列を線形に受け渡して書いた版より
約 130 倍遅い (20 万ステップで 28.6 s 対 0.22 s、`achc build -O2`)。結果は一致する。

## 再現手順

借用版のソースは `docs/artifacts/bench-0043/n-body-borrow.chs` (git 管理外。下の形のループ):

```haskell
advance :: Double -> MutRef r Bodies -> Borrow r ()
advance dt m = pairs 0 1 >> move 0
  where
    get k = U.index (asRef m) k
    add k d = get k >>= \v -> U.writeAt m k (v + d)
    ...
steps n b = case borrowMut b (advance 0.01) of (b', ()) -> steps (n - 1) b'
```

```sh
achc build -O2 -o nb n-body-borrow.chs && time ./nb 200000
```

## 期待される動作

借用を通した読み書きは、線形に受け渡す版と同程度 (Rust の `&mut [T]` 相当) の速さで動く。

## 原因

`-O2` の Core (`--dump=core-opt`) を調べた結果、次の三つが重なっていた。

1. **辞書が解決されない。** `Borrow r` の `Monad` インスタンスは型引数を取る辞書 `$fMonad = Λr. Monad$Dict …` で、
   フィールドが原子でない (`Λa b. …` やラムダ)。既知コンストラクタ表は原子フィールドの `Con` だけを登録し、
   `float_dict_fields` も `Λ` の下の `Con` を扱わないため、`>>=` のたびに `case ($fMonad @Unit) of Monad$Dict …` が残っていた。
2. **IO の bind がインライン化されない。** `primBindIO = cast prim$io_bind` は `cast` のため別名とみなされず、
   `(cast (λ…) T) a` も簡約されなかった。
3. **状態トークンについての η 展開がない。** `Borrow r a` (= `IO a` = `Unit# -> a` の `cast`) を返すループ
   (`pairs i j`) は、1 ステップごとに IO アクションのクロージャの木を作ってから実行する。GHC はここを
   「state hack」 (IO を返す関数を状態トークンについて η 展開する) で消す。

## 修正

- [x] 1: `float_dict_fields` が `Λtvs. K …` の非原子フィールドを `name$fN = Λtvs. field` に出し、`name$fN @tvs` で参照する。
  既知コンストラクタ表は `Λtvs. K ts atoms` を登録し、`g @ts` の `case` を型を代入して解決する
- [x] 2: `cast` の別名をインライン化し、`(cast (λb. e) (t1 -> t2)) a` を `cast (e[b := a]) t2` に簡約する。
  入れ子の `cast` はまとめ、`cast` の下の `let` は外に出す。`case (cast x T)` は `x` が `T` の同じ型引数の既知コンストラクタなら解決する
- [x] 回帰テスト: `tests/achc/run/h3/borrow_loop.hs` (`max-allocs`。修正前 870,118 → 修正後 660,106)、
  `cirka-core` の `opt_resolves_polymorphic_dictionary_under_type_application` / `opt_applies_through_casts`
- [x] 3: `io::io_arity` (06-ir.md §1.6、ADR-046): IO 的な型を返す関数 (トップレベルと局所 `letrec`) を、
  状態トークンを取る worker `f$io` と常にインライン化される wrapper に分ける。簡約器には、アトムの適用を `case` の選択肢へ押し込む規則、
  `cast` を `case` へ押し込む規則、1 回だけ使われるラムダをラムダの中へ移す規則、newtype の `case (cast x N)` の規則を加えた
- [x] 回帰テストの上限を下げる (`borrow_loop.hs`: 修正後 60,103 回、`max-allocs: 100000`)、
  `cirka-core` の `opt_gives_io_functions_the_state_token`

計測 (n-body 借用版、`-O2`):

| | 2 万ステップ | 20 万ステップ | 確保 (2 万ステップ) |
|---|---|---|---|
| 修正前 | 1.80 s | 28.6 s | 51.1M |
| 1・2 の後 | 1.27 s | — | 41.6M |
| 3 の後 | 0.11 s | 0.91 s | 3.8M |
| 線形な版 | — | 0.13 s | 760 (20 万ステップ) |

評価済みのサンクは 15.4M 回から 278 回になった。残りの約 7 倍の差は、bind を通る `Double` の箱 (1 ステップ約 190 回) によるもので、
FEAT-0057 で扱う。

## メモ
