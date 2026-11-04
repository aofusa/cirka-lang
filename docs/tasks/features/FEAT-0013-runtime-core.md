# FEAT-0013: ランタイム最小版

- 状態: todo
- マイルストーン: M1 バックエンド基盤
- 優先度: P0
- 依存: FEAT-0001
- 設計: [05-runtime.md](../../design/05-runtime.md)

## 概要

05-runtime.md のオブジェクト表現・アロケータ・解放・起動・パニック・出力を Rust (no_std) で実装する。

## 作業内容

- [ ] オブジェクトヘッダと kind 定義 (C ヘッダ `cirka_rt.h` も生成)
- [ ] `CIRKA_ALLOC=cirka|system` のアロケータ
- [ ] ワークリストによる反復的な `cirka_free_object`
- [ ] `main`: 大きなスタックのスレッド作成・ガードページ・終了処理
- [ ] バッファ付き stdout/stderr と `cirka_panic`
- [ ] debug ランタイム: マジック値・ポイズン・リーク報告

## 受け入れ条件

- [ ] Rust のユニットテストでアロケータと解放が正しい
- [ ] 100万要素のリストの解放でスタックが溢れない
- [ ] valgrind でエラー 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

