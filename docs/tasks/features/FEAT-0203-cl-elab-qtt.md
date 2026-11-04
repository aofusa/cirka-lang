# FEAT-0203: CirkaLang エラボレータ (QTT 型検査)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0202
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

双方向型検査による依存型 + 多重度 (0/1/ω) + 宇宙 Type/LType の型検査と、内部表現 TT への elaboration。トップレベルは型注釈必須。

## 作業内容

- [ ] TT (依存型付きコア) の定義と正規化 (NbE)
- [ ] 双方向型検査と単一化 (メタ変数)
- [ ] 多重度の検査 (QTT の使用量)
- [ ] U-Bind (LType は ω で束縛不可)
- [ ] データ型・パターンマッチ (case tree)
- [ ] 型エラー診断

## 受け入れ条件

- [ ] 簡単な依存型の例 (`Vect`) と所有権エラーのテスト
- [ ] TT の型検査器 (TT Lint)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

