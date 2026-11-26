# FEAT-1023: 演算ごとに動くランタイムの関数をテキストの先頭にまとめる

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-1019, FEAT-1022, FEAT-0091
- 設計: [07-backend.md](../../design/07-backend.md) §4, [16-performance.md](../../design/16-performance.md)

## 概要

ランタイム (`libcirka_rt.a`) の関数は、リンカがアーカイブのオブジェクトの順に置くので、テキストの中に散らばる。HTTP サーバの
1 要求では、生成コードの 4 ページのほかにランタイムの 6 ページ (`cirka_call_1` と `read_now`、`cirka_sock_recv` と `cirka_sock_send`、
`cirka_alloc` と `cirka_errno`、`cirka_async_watch` と `cirka_async_events`、`cirka_loop_run` と `cirka_bytes_from`、`cirka_free_object` と
`cirka_free_raw`) を実行する (Rust は合わせて 7 ページ)。要求の処理はシステムコールの間の短い区間で、カーネルの処理の後は TLB と
キャッシュが冷えているので、ページごとに引き直しがかかる。

演算ごとに動く関数 (確保と解放・RC の遅い経路・`force`・部分適用・イベントループとそれが動かすソケットの I/O・`errno`) を
`.text.hot.<名前>` の節に置き、生成コードのクロージャの呼び出し (`cirka_apply_N`・`cirka_call_1`) に LLVM の `hot` 属性を付ける
(LLVM は `.text.hot.` の節に置く)。GNU ld の既定のリンカスクリプトは `.text.hot .text.hot.*` を他のテキストより前にまとめる。

## 作業内容

- [x] 設計: 07-backend.md §4 (リンクのコマンドを実際のものに直し、コードの配置を書く)、16-performance.md の表
- [x] `cirka-runtime`: 演算ごとに動く関数に `#[unsafe(link_section = ".text.hot.<名前>")]`
- [x] `cirka-codegen-llvm`: `cirka_apply_N` と `cirka_call_1` に `hot`
- [x] テスト: リンクしたサーバのプログラムで、使う熱い関数の最初から最後までの間に他の関数がないこと (`nm -n`)
- [x] 計測: HTTP サーバの 1 要求の命令数・実行するコードのページ・CPU 時間、全ベンチ

## 受け入れ条件

- [x] HTTP サーバの 1 要求で実行するランタイムのコードのページが減り、応答が変わらない
- [x] 全ベンチの出力が変わらず、遅くなるベンチがない (binary-trees は FEAT-1028 までの最終の計測で。メモ)
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ

- 計測 (Docker、i5-3320M、FEAT-1022 との交互の 5 回の中央値。`docs/artifacts/f1026`): HTTP サーバの 1 要求の命令数 906 → 905、
  実行するコードのページ (lackey) 10.1 → 6.0、コードのキャッシュライン 90.0 → 80.7 本。スループット 0.999 倍 (16 接続)・
  0.996 倍 (64 接続) で、ページは減ったが速さは誤差の範囲だった。
- binary-trees は FEAT-1017 の 1.019 倍 (FEAT-1022) から 1.100 倍に遅くなった (命令数は同じ 15.2 億)。原因は調べていない
  (演算ごとのランタイムの関数の位置と、生成コードの位置が変わった)。確保と解放を生成コードでインラインにした FEAT-1027 の後は
  呼ばれなくなり、FEAT-1028 までの最終の計測で FEAT-1017 より速い (FEAT-0091 のメモ)。
