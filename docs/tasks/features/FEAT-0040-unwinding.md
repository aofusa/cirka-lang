# FEAT-0040: 例外/パニックのアンワインド

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0031
- 設計: [05-runtime.md](../../design/05-runtime.md), [07-backend.md](../../design/07-backend.md)

## 概要

Itanium ABI のアンワインドと landing pad での drop/release。aCHC の `catch`/`bracket`、CirkaLang の panic=unwind の共通基盤。

## 作業内容

- [ ] `invoke`/landing pad の生成
- [ ] ランタイムの raise / catch
- [ ] W0305 (例外経路のリーク)

## 受け入れ条件

- [ ] 例外発生時に解放処理が実行されることを出力で確認
- [ ] 例外経路でもリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

