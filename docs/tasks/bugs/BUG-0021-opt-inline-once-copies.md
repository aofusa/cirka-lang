# BUG-0021: インライン化で、常にインライン化される関数を経由して 1 回だけ呼ばれる関数が指数的に複製される

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0241 (`tests/cirkalang/run/v005/http_echo.cirka` のコンパイルが終わらない)
- 関連: FEAT-0033, FEAT-0041, FEAT-0071, FEAT-0241

## 現象

継続を渡して書いた HTTP エコーサーバのサンプル (`http_echo.cirka`) の `cirka check` に、リリースビルドで約 60 秒、
デバッグビルドでは数分かかる (各パスの後の Core Lint が大きな式を何度も検査する)。
最適化後の Core は 14 MB (最適化前は 392 KB) で、`Main.main` の中に接続の処理が 114 回複製されていた。

最適化の各段の後の Core の大きさ (ノード数) を数えると、worker/wrapper の後の最初の simplify で 30,588 から 1,990,666 に増える。

## 再現手順

```sh
target/release/cirka check tests/cirkalang/run/v005/http_echo.cirka
```

Core では、大きい関数 `g{k}` が小さい関数 `f{k+1}` を 2 回呼び、`f{k}` は `g{k}` を呼ぶだけ、という連鎖で起きる
(単体テスト `opt_bounds_the_copies_made_by_inlining`)。

## 期待される動作

インライン化で増えるコードは、呼び出しの数に比例する程度にとどまる。

## 原因

`opt.rs` の `inline_candidates` は「1 か所からしか呼ばれない」を、プログラムに今ある出現の数で判定していた。
ところが簡約器は、適用位置にインライン化した本体をその場で簡約し直し、その中の候補もインライン化する。
インライン化される関数 (worker/wrapper の wrapper、`INLINE_SIZE` 以下の小さい関数、1 か所からしか呼ばれない関数) の中の呼び出しは、
その関数をインライン化した箇所の数だけ複製される。wrapper の中で 1 回だけ呼ばれる worker は、wrapper の呼び出し箇所すべてに
インライン化され、その worker が別の wrapper を 2 回呼ぶとその先の worker も倍になる。呼び出しの深さに対して指数的に増える。

`http_echo.cirka` では、`exchange` → `connect` → `sendAll` → … と継続を受け取る関数が、成功と失敗の両方の分岐から次の関数を呼ぶ。
worker/wrapper がこれらを分割した直後の simplify で、連鎖全体が `main` に展開された。

## 修正

- [x] 回帰テストを追加 (`crates/cirka-core/src/tests.rs` の `opt_bounds_the_copies_made_by_inlining`。
      修正前の最適化器では 10 段の連鎖が 827 ノードから 77,750 ノードになり、表明が失敗する。
      `http_echo.cirka` の実行テストも、修正前はコンパイルが終わらない)
- [x] 1 か所からしか呼ばれない関数は、インライン化した後の複製の数を数え、増えるノード数
      (大きさ × (複製の数 − 1)) が `COPY_BUDGET` = 500 以下のときだけインライン化する (`Copies`)
- [x] ベンチの最適化後の Core と LLVM IR を修正前と比べ、違うものは命令数を計測する

## メモ

**修正。** 複製の数は、出現ごとに、それを含む関数がインライン化されるならその関数の複製の数、されないなら 1 を足したもの。
インライン化されるかどうかは呼び出し元の側から決まるので、呼び出し元へ向かってメモ化しながら辿る
(ループ破りは複製されないので辿りは止まる。wrapper と worker が呼び合う閉路は 1 と数える)。
小さい関数と wrapper は従来どおり常にインライン化する。2 か所以上から呼ばれる大きい関数も従来どおりインライン化しない。

**複製をすべて禁じる案は採らなかった。** はじめは「複製の数がちょうど 1」に限ったが、wrapper 経由の 2〜5 個の複製で
性能が出ていた関数 (string-split の `Main.step$w`・`Data.String.appendRange$w` など) がすべて呼び出しのままになった。
予算 500 は、ベンチで複製されていたもの (大きさ 65〜185、複製 2〜5 個) がすべて入る値。

**予算を 1 か所から呼ばれる関数に限った理由。** 2 か所以上から直接呼ばれる関数にも予算内のインライン化を許すと、
aCHC のベンチがすべて別のコードになった。新しい最適化になるので、この修正には含めない。

**計測 (修正前 = 作業ツリーから `opt.rs` だけを戻したもの、2026-11-14)。**

- `http_echo.cirka`: 最適化後の Core のダンプ 14 MB → 179 KB、`cirka check` はリリースビルドで約 60 秒 → 1.6 秒、
  デバッグビルドで数分 → 4.2 秒。
- ベンチ 17 本 (array-sort / binary-trees / cli-grep / fannkuch-redux / mandelbrot / n-body / spectral-norm / string-split の
  `main.cirka`・`own.chs`・`plain.hs` のうち aCHC と CirkaLang で作れるもの。Z3 の要る binary-search はホストで作れないので除く) の
  `-O2` の最適化後の Core と LLVM IR は、array-sort の `own.chs` 以外すべて同一 (変数番号を除く)。
  array-sort `own.chs` は結果の表示に使う `showsPrec` の worker が呼び出しのまま残り、
  命令数 (callgrind) は 294,463,001 → 294,463,869 (+0.0003%)。
- 「複製の数がちょうど 1」に限った版では、string-split `main.cirka` の時間が 7 回の中央値で +12.6%、cli-grep `main.cirka` が +2.8% だった。

**試して入れなかったもの。** 静的な関数引数の特殊化 (`sat.rs`) の複製の中で複数回呼ばれるラムダを大域関数にする変更を先に試したが、
大きさは変わらなかった (複製の原因はインライン化だった) ので入れていない。
