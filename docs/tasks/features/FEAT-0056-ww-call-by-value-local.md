# FEAT-0056: worker/wrapper の拡張 (値渡しの引数・局所関数)

- 状態: todo
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

- [ ] 値渡しの宣言と、それを使う unboxing
- [ ] 局所関数の worker/wrapper (またはラムダ持ち上げ)
- [ ] ベンチの再計測 (mandelbrot / spectral-norm / n-body)

## 受け入れ条件

- [ ] `bench/mandelbrot/main.cirka` の内側のループが確保をしない (`CIRKA_STATS=1`)
- [ ] `bench/mandelbrot/plain.hs` の内側のループがサンクを作らない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
