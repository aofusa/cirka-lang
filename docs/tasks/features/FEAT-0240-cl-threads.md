# FEAT-0240: スレッドと共有

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P0
- 依存: FEAT-0235, FEAT-0045
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)、[spec-v0.0.5.md](../../design/cirkalang/spec-v0.0.5.md) §1、ADR-088

## 概要

スレッド生成、`Send`/`Sync` 相当のトレイト (スレッド間で渡せる型の区別)、Mutex・チャネル。

## 作業内容

- [x] スレッド API: `Std.Thread` の `spawn` / `join` / `threadId` / `yield` / `cpuCount` (ランタイムの `cirka_thread_*`、FEAT-0045)
- [x] スレッド安全性のトレイトと自動導出: `Send` (`impl` は書けず型の構造から決まる、E1259 / E1261)、`ThreadBound` の印。
      `Sync` は持たない (ADR-088)
- [x] 同期プリミティブ: `MVar` / `Chan` / `Mutex` (`Mutex` は `MVar` の上のライブラリ)
- [x] テストの仕組み: CirkaLang の実行テストの `skip-backend: interp`、`tests/cirkalang/fail/thread/`。
      リーク検査を他のスレッドの実行中で飛ばしたときのランタイムの通知

## 受け入れ条件

- [x] データ競合しない並行テスト (TSan): `tests/cirkalang/run/v005/threads.cirka` (`tsan: 1`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 2026-11-14: 設計は spec-v0.0.5.md §1 と ADR-088。
- 実装中に決めたこと:
  - `Send` は Prelude のメソッドのないトレイトで、インスタンス探索の中で型の構造から解く (`cirkalang-elab` の `elab/send.rs`)。
    辞書はフィールドのない構築子で、生成コードに残らない (`run/v005/threads.cirka` の LLVM IR に `Send` は現れない)。
  - 添字付きの型族: `Vect n a` の `a` は構築子の結果の型が `VCons` のフィールドをそのまま置く添字なので、実際の引数で置き換える。
    最初は型パラメータだけを置き換え、`Vect` のフィールド `a` が存在型として `Send` でなくなった。
  - 再帰的な出現の仮定: データ型の名前だけで仮定すると、大きな消去 (`if b then ThreadBound else Int`) で添字ごとに `Send` かどうかが
    変わる型を誤って `Send` にする。引数ごとに比べると `Vect` の再帰 (`Vect n' a`、`n'` は毎回新しいフィールドの値) が止まらない。
    フィールドの未知の値はどれも同じに扱われるので、未知の値どうしを等しいとみなして引数を比べる (健全で、`Vect` も 1 段で止まる)。
    引数が大きくなり続ける型は 16 段で `Send` でないとする (Rust も同じ型で要求の評価があふれる)。
  - 比べる引数にまだ解けていないメタ変数 (整数リテラルの型) があると、同じメタ変数を別のものとみなして 16 段まで潜っていた
    (`newMVar (Node Leaf 1 Leaf)` が E1261)。同じメタ変数は等しいとみなし、フィールドで止まって後で解き直す。
  - 存在型のフィールドは `Send` でない。ただし多重度 0 なら実行時に何も残らないので調べない (`run/v005/send.cirka` の `Tagged`)。
  - `MVar`・`Chan`・`spawn` は型 `a` の値を渡すのでプリミティブにし、`threadId`・`yield`・`cpuCount` は `extern "cirka"` で書いた。
    `tryTakeMVar`・`tryRecv` は空のときの既定値に `MVar`・`Chan` 自身を渡し、ポインタの比較で `Maybe` にする。
  - データ型の引数は既定で `LType` なので、`Mutex` は `data Mutex (a : Type)` と書く。
- テストの仕組み:
  - インタプリタを飛ばすテストは、これまで「確保 = 解放」を求めていた。CirkaLang のプログラムは CAF (`main` など) を解放しないので
    常に合わない。デバッグ用ランタイムの実行 (リーク検査つき) で残った数を基準にし、ほかの構成も同じ数を残すことにした。
    確保の数そのものを比べると `tests/mir/run/threads.mir` の TSan + system アロケータの構成で数が実行ごとに変わり失敗した
    (CAS のやり直しなどスレッドの進み方による)。
  - リーク検査は他のスレッドが走っていると黙って飛ばしていたので、テストで検査されないまま通りうる。ランタイムが
    `cirka: leak check skipped: N threads still running` と知らせ、ハーネスはこれを失敗にする (`tests/mir/exit/running_thread.mir`)。
- `run/v005/threads.cirka` (8 スレッドが `Mutex` の数を 1000 回ずつ増やし、`Chan` でリストを送り、`join` で組を返す。`MVar` 2 つの
  ピンポン 1000 往復) の TSan・TSan + system・release の実行ファイルを各 30 回実行し、競合の報告・リーク・数の違いなし。
- Docker のコンテナの `scripts/ci.sh` で、ThreadSanitizer の段階 (`threads.cirka` の TSan の 2 構成を含む) まで通った。
