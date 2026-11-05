# FEAT-0056: worker/wrapper の拡張 (値渡しの引数・局所関数)

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0041
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

FEAT-0043 のベンチ (`bench/mandelbrot`、[bench-report.md](../../design/bench-report.md)) で見つかった unboxing の取りこぼし。

1. **値渡しの引数**: worker/wrapper は正格性解析で正格と分かった引数だけを unboxing する。CirkaLang は正格 (値渡し) なので
   引数は呼び出し時に必ず値だが、本体のすべての経路で使われない引数 (例: mandelbrot の `go k zr zi cr ci` は `k >= 50` の枝で
   `zr` などを使わない) は箱のまま渡され、反復ごとに `F#` を 2 つ確保する (Rust の約 30 倍遅い)。
   Core の `Program` に「関数の引数は常に WHNF の値」というフロントエンドに依存しない性質を宣言する手段を設け、
   その場合は箱型の引数をすべて unboxing する。
2. **局所関数**: worker/wrapper は最上位の関数だけが対象。aCHC の `where` の局所ループ (`go`) は対象外で、
   mandelbrot の `plain.hs` は反復ごとにサンクと箱を作る (`CIRKA_STATS` で forced が約 200 万)。
   局所関数のラムダ持ち上げ、または局所関数の正格性解析と worker/wrapper を行う。

## 作業内容

- [x] 評価済みの引数の unboxing (値渡しの宣言の代わりに、呼び出し側の解析で行う。フロントエンドに依存しない)
- [x] 局所関数: λ持ち上げを外側の束縛群から行い、持ち上げた関数の型・辞書の引数を先頭に置いて特殊化の対象にする
- [x] ベンチの再計測

## 受け入れ条件

- [x] `bench/mandelbrot/main.cirka` の内側のループが確保をしない (`CIRKA_STATS=1`: n=200 で 413 万回 → 5.6 万回)
- [x] ~~`bench/mandelbrot/plain.hs` の内側のループがサンクを作らない~~ → 局所ループ (`rows`/`cols`) が持ち上げ・特殊化される
  (下記メモ。残るサンクは Haskell の意味から来るもので、意味を変えずには除けない)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計: [06-ir.md](../../design/06-ir.md) §1.4「評価済みの引数」、§1.5 λ持ち上げ。
- 方針の変更: 当初は Core に「値渡し」を宣言する手段を設ける予定だったが、呼び出し側を見て「すべての呼び出しで評価済み (または安全に先に評価できる)」
  引数を求める解析にした。CirkaLang ではほぼすべての引数が該当し、aCHC の値で渡されるループの引数にも効き、フロントエンド固有の情報が要らない (原則 6)。
- mandelbrot の `plain.hs`: `rows`/`cols` は `NoMonoLocalBinds` で `Num a` に一般化されていて、内側の `cols` が外側に値として渡されるため
  持ち上げも特殊化もされていなかった (修正済み)。残るサンクは `go` の `zr`/`zi` の引数で、`cr`/`ci` が `fn` (= `read` した引数から作る、
  失敗しうる遅延束縛) に依存するため先に評価できない。`go` は `k >= 50` の枝で `zr` を使わないので正格でもない (GHC も同じ判断になる)。
- 計測 (アイドル状態、`/usr/bin/time`、変更前 → 変更後):

  | 実装 | 引数 | 変更前 | 変更後 |
  |------|-----:|------:|------:|
  | CirkaLang mandelbrot | 1000 | 1.99 s | 0.15 s |
  | CirkaLang spectral-norm | 1000 | 2.58 s | 0.75 s |
  | CirkaLang n-body | 1,000,000 | 2.90 s | 1.12 s |
  | aCHC spectral-norm `plain.hs` | 1000 | 1.79 s | 0.45 s |
  | aCHC array-sort `own.chs` | 1,000,000 | 1.17 s | 0.54 s |
  | aCHC n-body `own.chs` | 1,000,000 | 0.71 s | 0.45 s |
  | aCHC mandelbrot `plain.hs` | 1000 | 2.64 s | 2.45 s |
  | その他 (binary-trees, fannkuch-redux, n-body `plain.hs`, spectral-norm `own.chs`) | | | 変化なし (±3%) |

  Rust は mandelbrot 1000 で 0.12 s、spectral-norm 1000 で 0.20 s (FEAT-0043 の計測)。
