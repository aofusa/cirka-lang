# FEAT-0161: 効果システム (Eff es a)

- 状態: todo
- トラック: aCHC
- マイルストーン: H7 効果・能力・セッション型 (CirkaLang v0.0.7 の後)
- 優先度: P1
- 依存: FEAT-0263, FEAT-0142, FEAT-0143
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

DataKinds の効果列を持つモナド `Eff es a`、組み込みの所属制約ソルバ、evidence passing によるハンドラのコンパイル (CirkaLang の変換を再利用)。

## 作業内容

- [ ] 効果の宣言 (GADT) と send
- [ ] `e :> es` の組み込みソルバ
- [ ] ハンドラ (State / Reader / Error / Console など)
- [ ] 共通 Core 上の evidence passing 変換の利用
- [ ] 線形値と継続の多重呼び出しの制限

## 受け入れ条件

- [ ] State 効果のベンチが手書きの状態渡しと同等
- [ ] 効果の型エラーテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

