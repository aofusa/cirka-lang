# FEAT-0054: Windows ターゲットへの移植

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0052, FEAT-0045
- 設計: [15-gpu-and-remote.md](../../design/15-gpu-and-remote.md)

## 概要

PE/COFF 出力 (x86_64-pc-windows-msvc) とランタイムの Win32 移植。アンワインドは当面 abort。

## 作業内容

- [ ] ターゲットの切り替えとリンク
- [ ] VirtualAlloc・スレッド・コンソール I/O・スタックサイズ
- [ ] 遠隔でのテスト実行

## 受け入れ条件

- [ ] 遠隔の Windows で run テストの主要部が通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

