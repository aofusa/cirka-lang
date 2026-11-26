# BUG-0044: デバッグ用ランタイムでの -O0 のビルドが、使わないスレッドのブロックの宣言のためにリンクできない

- 状態: done
- 重大度: major
- 発見箇所: FEAT-1029〜1031 を main に入れた後の `scripts/ci.sh` (`bindgen_run`)
- 関連: FEAT-1027, FEAT-0091

## 現象

`--runtime=debug` での `-O0` のビルド (`bindgen_run` の 5 題材すべて) がリンクで失敗する。

```
/usr/bin/ld: cirka_tls_block: TLS definition in .../rt-debug/release/libcirka_rt.a(cirka_rt-....o) section .tbss
  mismatches non-TLS reference in /work/target/tmp/out-....o
/usr/bin/ld: .../rt-debug/release/libcirka_rt.a: error adding symbols: bad value
```

`cargo test` は失敗したテストのバイナリで止まるので、同じ組み合わせでビルドする `both_clis_agree_on_core_input` (tests/driver.rs) などは
実行されていない。

## 再現手順

```sh
achc run -O0 --runtime=debug --build-dir=... tests/bindgen/run/counter/Main.hs   # bindgen で作ったモジュールと C の補助を付けて
```

## 期待される動作

`-O2` と同じく、リンクして実行できる。

## 原因

FEAT-1027 で、生成コードがランタイムのスレッドのブロック `@cirka_tls_block` (隠れた可視性、local-exec の TLS) を直接読むようになり、
その宣言をモジュールに必ず出していた。デバッグ用ランタイムでは確保とエラー番号をインライン化しないので、ブロックを読むコードが
1 つもない。LLVM は使われない隠れた宣言にも未定義のシンボルを書き (`NOTYPE GLOBAL HIDDEN UND cirka_tls_block`)、型が TLS でないので、
GNU ld はランタイムの TLS の定義と合わないとして拒む。使われるときは TLS の再配置から型が TLS になるので、`-O2` はリンクできる。

## 修正

- [x] 回帰テストを追加 (`cirka-codegen-llvm`: デバッグ用ランタイムの IR と、ブロックを読まないプログラムの IR が `@cirka_tls_block` を含まない)
- [x] 修正: ブロックを読む関数があるときだけ宣言する (`Gen::uses_tls`)
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ

- 修正の後、`scripts/ci.sh` が通る (538 件、`bindgen_run` の 5 題材と `both_clis_agree_on_core_input` を含む)。
