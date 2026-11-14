# FEAT-0235: CirkaLang v0.0.4 リリース

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P0
- 依存: FEAT-0233, FEAT-0234
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

証明機能の評価 (書き味・検証時間・性能への影響なし) とリリース。

## 作業内容

- [x] 検証済みサンプル (ソート・境界安全な配列処理・パーサ): `tests/cirkalang/verify/pass/sample_sort.cirka` / `sample_array.cirka` / `sample_parser.cirka`
- [x] CHANGELOG
- [ ] タグ `cirkalang-v0.0.4` (利用者の確認を得てから打つ。AGENTS.md)

## 受け入れ条件

- [x] 証明の有無で実行性能が変わらないことを記録 (bench-report.md の「CirkaLang v0.0.4」: 証明のある/ないプログラムの MIR の一致、
      証明を使わないベンチの LLVM IR が v0.0.3 と同じこと、証明で検査を除いた binary-search が 4〜6% 速いこと)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 版: `Cargo.toml` のワークスペースの版、`cirka` の `EDITION`、パッケージのテストの `edition`、E1004 の文言を 0.0.4 にした。
- サンプルを書く中で検証器を補った: 部分集合型の結果を持つ呼び出しを、展開せず 1 つの変数 (stand-in) として扱う (相互再帰する構文解析で検証が終わらなかった)。
  結果がレコードのとき、パターンの変数をフィールドの射影と結び付ける。配列・文字列の大きさは 0 以上。`{x : A | p} -> B` は `B` でも `x` を使える。
- 見つけたバグ: BUG-0017 (トレイトのメソッドを使う不変条件の ICE と W1231)、BUG-0018 (検証が `Int` のラップを無視。検査の除去でメモリ安全が破れていた。critical)、
  BUG-0019 (メタ変数の表のずれによる ICE)。
- 後続: FEAT-0244 (core の配列・文字列の関数に大きさの契約)。
- 評価メモは 12-cirkalang.md の「v0.0.4 の評価」。
