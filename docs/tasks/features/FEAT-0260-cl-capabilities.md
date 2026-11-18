# FEAT-0260: ケイパビリティ

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.7 効果・能力・セッション型
- 優先度: P1
- 依存: FEAT-0253
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

能力トークンで副作用の種類を型に表す (ファイル・ネットワーク・時刻・乱数)。

## 作業内容

- [x] v0.0.7 の仕様 `docs/design/cirkalang/spec-v0.0.7.md` (v0.0.6 からの差分) を作り、版表示を `v0.0.7-dev` にする
      (ワークスペースの `version = "0.0.7-dev"`。AGENTS.md §1「版の運用」。v0.0.6 までは開発中も前の版のままだった)
- [x] Cap 型と取得方法
- [x] std の能力付き API
- [x] 能力を持たない関数での使用禁止

## 受け入れ条件

- [x] 能力のない関数がファイルを読めない (型エラー)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計 (spec-v0.0.7.md §1、ADR-103): 能力 `Cap k` (`k : Capability` = `FileSystem` `Network` `Clock` `Random` `Process` `Environment`) は
  構成子のない共有できるトークンで、`main` とテストの引数としてだけ得る。標準ライブラリは、外の資源に名前で触れる操作と時刻・乱数の観測に、
  能力を多重度 0 で最初の引数に取る。持っているハンドル・標準入出力・引数・スレッドの操作は能力を取らない。
  13-verification.md §4.1 の当初案 (線形なトークンを操作ごとに返す) は、安全性が同じで書き方が重いので採らなかった。
- 偽造の防止: 消去される位置の能力は消去される値の全域性 (BUG-0033、E0507、ADR-102) で守る。この作業中に、`primRewrite` に `panic` の
  証明を渡すと任意の型を変換できる (能力も `()` から作れる) ことが分かり、先に BUG-0033 として直した。`extern` ブロックは信頼の境界として制限しない。
- 新しい診断: E1117 (テストの引数が能力でない)。E1200 は `main` の能力の引数を許し、期待した型が `Cap k` の不一致には能力の受け取り方をヘルプに出す。
- 追加: `Std.Random` (`randomInt`・`randomBelow`。ランタイム `cirka_random_u64` は `getrandom`、インタプリタは `/dev/urandom`)。
- テスト: `fail/cap/` (能力のない関数・別の種類の能力・偽造 3 通り・`main` の型)、`fail/test/param.cirka` (E1117)、
  `run/v007/capabilities.cirka` (記録・クロージャ・乱数・環境変数)、`run/v007/cap_thread.cirka` (スレッドに送る)、
  `run/v007/cap_erased.cirka` と `cap_none.cirka` (`-O2` で同じ MIR。`cirkalang_capabilities_are_erased`)、`test/caps.cirka` (テストが能力を受け取る)。
- 書き換え: std の操作を使うテスト 11 本とベンチ 2 本に能力を通した (非互換。FEAT-0263 と CHANGELOG に記録する)。
- spec-v0.0.7.md §4 に対象外 (能力の弱め方・取り消し、パッケージ単位の `extern` の禁止、aCHC への取り込み) を書いた。
- コスト (`cirka build -O2 --emit=llvm`、ホスト): 多重度 0 の能力を渡す `cap_erased` と能力のない `cap_none` は LLVM IR まで同一。
  能力を ω にすると MIR と LLVM IR の関数に引数が 1 つ増えるが、clang `-O2` の後には残らない (定数の引数が伝播し、関数がインライン化される)。
  能力を受け取る `main` は関数になるので、この例では `cirka_main` が 48 行から 40 行になった (`main : IO ()` はトップレベルの値として評価する)。

