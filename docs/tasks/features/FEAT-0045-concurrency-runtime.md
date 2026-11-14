# FEAT-0045: 並行性ランタイム (スレッド・アトミック RC)

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0031
- 設計: [04-memory-model.md](../../design/04-memory-model.md), [05-runtime.md](../../design/05-runtime.md)

## 概要

OS スレッド、スレッド間共有時の共有マークとアトミック RC、同期プリミティブ。

## 作業内容

- [x] `cirka_mark_shared` (到達できる正の RC をすべて負にする。作業スタックは `malloc`。同期オブジェクトは中身も辿る)
- [x] 負の RC のアトミック操作 (dup は Relaxed の加算、drop は AcqRel の減算)。生成コードの高速経路は RC の符号を見て、負ならランタイムを呼ぶ
- [x] スレッド生成・join (`cirka_thread_spawn`/`join`、pthread の切り離されたスレッド、生成コードの C ABI の入口 `cirka_call_1`)、
      スレッド番号・`yield`・CPU 数
- [x] 同期オブジェクト (kind `MVAR`: `MVar`・チャネル・スレッドのハンドル。pthread の mutex と条件変数)、`MUTVAR` の CAS
- [x] 共有されたサンクの評価 (CAS で `BLACKHOLE` にしたスレッドだけが評価し、ほかは `IND` を待つ)
- [x] スレッドごとの状態 (`global_asm!` の TLS ブロック): フリーリスト・統計・スレッド番号・スタックの範囲。終了したスレッドのフリーリストのプール
- [x] ThreadSanitizer でのテスト (`--runtime=tsan`、テストのヘッダ `tsan: 1`)
- [x] MIR インタプリタ: `MVar`・チャネルを単一スレッドの意味で実装 (`spawn` は実行時エラー)

## 受け入れ条件

- [x] 並行テストが TSan でエラーなし (ホスト。Docker のコンテナの中では FEAT-0084)
- [x] 単一スレッドの性能が低下しない (下記の計測)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

### 実装

- 設計は 05-runtime.md §9、04-memory-model.md §2.4、ADR-086。ランタイムの新しいモジュールは `thread.rs` (生成と join)・`sync.rs`
  (MT の印・スピンロック・同期オブジェクト)・`tls.rs` (TLS ブロック)。
- 単一スレッドの費用を払わない工夫: 共有されたオブジェクトだけがアトミック、入出力と debug ランタイムの生存表のロックは MT のときだけ、
  `cirka_force` はレジスタを退避しない (遅い経路は別の関数)、確保の高速経路は葉の関数、生成コードのヘッダの読み書きは `unordered` の
  アトミック (命令は同じ `mov`。ストアもアトミックにしないと LLVM の store-to-load forwarding が止まる)。
- テスト: ランタイムの単体テスト (`spawn_and_join`・`mvar_and_channel_between_threads`・`shared_thunk_is_evaluated_once`・
  `shared_mutvar_compare_and_swap`・`mark_shared_reaches_deep_structures`・`every_thread_has_its_own_allocator_state`・
  `objects_freed_by_another_thread`)、MIR の実行テスト `tests/mir/run/threads.mir` (`tsan: 1`。4 スレッドの `MVar` のカウンタ・チャネル・
  共有されたサンク・`IORef` の CAS・長いリストを複数のスレッドで合計)、`mvar.mir` (インタプリタとネイティブの一致)、
  `mvar_blocked.mir` (待ち続ける `take` は GHC と同じ文言で終了)。
- TSan: ホスト (LLVM 18) で `cargo test --release -p cirka-integration --test mir` が TSan の 2 構成 (既定のアロケータと `CIRKA_ALLOC=system`)
  を含めて通る。TSan の構成が実際に検査していることは `tsan_reports_a_data_race` (`tests/mir/tsan/race.mir`: 2 スレッドが同期なしに
  配列の要素を読み書きする) で確かめる。TSan は実行で起きた競合だけを報告し、共有されたオブジェクトのアトミックな RC が先行関係を作ることがあるので、
  どの実行でも 2 つのアクセスが順序づかない形にした (20 回中 20 回報告)。Docker のコンテナの中では seccomp のために TSan が起動できず、
  FEAT-0084 で扱う。

### 単一スレッドの性能

詳細は bench-report.md「並行性ランタイムの単一スレッドの性能 (FEAT-0045)」。前 = `920a682`。

- 命令数 (callgrind。既定のアロケータの予約を 4 GiB にした計測用のランタイム): 17 のベンチすべてで前と同じか少ない (最大 +0.04%)。
  binary-trees 0.898〜0.943、spectral-norm `plain.hs` 0.916、fannkuch-redux `plain.hs` 0.955、cli-grep 0.988、string-split 0.989。
- 時間 (詰め物 0/16/32/48 バイトの 4 通りのリンクの平均): binary-trees 0.877〜0.905、spectral-norm `plain.hs` 0.854、
  fannkuch-redux `plain.hs` 0.935、ほかは 0.98〜1.04。cli-grep (1.036) と string-split (1.022) は 15 回の再計測で 0.975 と 1.019 で、
  命令数は減っているので揺らぎと判断した。
- 速くなったのは、単一スレッドの費用を相殺するために高速経路を見直したため: 確保の高速経路を葉の関数にし (遅い経路は `#[inline(never)]`)、
  `cirka_force` はレジスタを退避せずに評価の関数へ跳び、解放の経路から外部資源 (`FOREIGN`・`MVAR`) の後始末を外した。

調査で分かったこと:

- valgrind の下では 64 GiB の `MAP_NORESERVE` の予約が失敗し、ランタイムは `malloc` に切り替わる。callgrind の命令数で既定のアロケータを
  比べるには、予約を小さくしたランタイムが要る (10-testing.md §5 に記録)。
- 生成コードがヘッダのロードだけを `unordered` のアトミックにし、ストアを普通のままにすると、LLVM が書いたばかりの RC を読み直す
  (cli-grep の主要な関数が大きくなった)。ストアも `unordered` にして元に戻った (05-runtime.md §9.3)。
- cli-grep は関数の番地が 16 バイトずれるだけで ±8% 変わる。ランタイムに `#[cold]` の関数を足すと `.text.unlikely` が先頭に置かれて
  生成コードがずれ、TLS のフリーリストが遅いように見えた。詰め物で配置を揃えると差はなかった (FEAT-0085 を起票)。
- フリーリストを単一スレッドの間だけ大域に置く方式は、MT の判定の分岐のために binary-trees で 13% 遅く、採らなかった。

### 起票

- FEAT-0084: Docker の CI で ThreadSanitizer を動かす。
- FEAT-0085: コード配置による性能の揺らぎの抑制。
