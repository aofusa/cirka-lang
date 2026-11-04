# FEAT-0013: ランタイム最小版

- 状態: done
- トラック: 共通基盤
- マイルストーン: M1 バックエンド基盤
- 優先度: P0
- 依存: FEAT-0001
- 設計: [05-runtime.md](../../design/05-runtime.md)

## 概要

05-runtime.md のオブジェクト表現・アロケータ・解放・起動・パニック・出力を Rust (no_std) で実装する。

## 作業内容

- [x] オブジェクトヘッダと kind (C ヘッダ `cirka_rt.h` も生成)
- [x] `CIRKA_ALLOC=cirka|system`
- [x] ワークリストによる反復的な解放
- [x] `main`: 大きなスタックのスレッド・ガードページ・終了処理
- [x] バッファ付き stdout/stderr と `cirka_panic`
- [x] debug ランタイム (マジック値・ポイズン・リーク報告)

## 受け入れ条件

- [x] アロケータと解放のユニットテスト
- [x] 100万要素のリスト解放でスタックが溢れない
- [x] valgrind でエラー 0
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- アロケータ (`cirka`): 64GiB の仮想領域を `MAP_NORESERVE` で予約し、64KiB ページ単位でサイズクラス (8〜256B) に割り当てる。ページごとのサイズクラス表により、ヘッダにサイズを持たずにアドレスだけで解放先のクラスが分かる。256B 超は `malloc`。
- 解放のワークリストは死んだオブジェクト自身のヘッダ (8 バイト) に「次へのポインタ >> 3 (44bit)・kind・nptr」を詰めて連結する (追加メモリなし)。100 万要素のリスト解放のテストあり。
- debug ランタイム (`--features debug-rt`、`target/rt-debug`) は 48 バイトの接頭部 (マジック値・確保番号・生存リスト) を付けて `malloc` し、解放時にポイズン + 4096 個の検疫。`CIRKA_LEAK_CHECK=1` で CAF から到達できない生存オブジェクトを報告。
- `CIRKA_STATS=1` で確保/解放回数を表示 (インタプリタとの比較に使う)。
- スタックオーバーフローは代替シグナルスタック上の SIGSEGV ハンドラでガード領域への fault を判定し `stack overflow` (終了コード 2)。
- precompiled `core` が `rust_eh_personality` を参照するため空の定義を置く。
- valgrind (`CIRKA_ALLOC=system`) でエラー 0 を Docker 内で確認 (C から呼ぶスモークテスト)。
