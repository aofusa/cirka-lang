# FEAT-0079: 文字列処理のループの箱の除去

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0022, FEAT-0056, FEAT-0061, FEAT-0066
- 設計: [16-performance.md](../../design/16-performance.md) §4

## 概要

CirkaLang の `Data.String` (FEAT-0211) で書いた `bench/string-split` は、1 行ごとにビュー `Str`・`Builder`・`Maybe Int` の箱を作り直し、100 万行で 2400 万回確保していた (Rust の 4.8 倍)。
フロントエンドに依存しない Core / MIR の最適化で箱を除く。

## 作業内容

- [x] `sat.rs`: 静的な関数引数の特殊化 (FEAT-0061) を多相関数に広げる。写しは呼び出しの型引数で単相化する (`foldLines` などに渡す関数の箱を消す)
- [x] `ww.rs`: 積型の引数の worker/wrapper。分解する (`case` の対象になる) だけの単一構成子の引数をフィールドに分ける (`Str` を 3 つの値で渡す)。自己末尾呼び出しで同じ形を渡すループも対象
- [x] `opt.rs`: 箱型 (`I#` など) の `let!` の右辺が case なら、各末尾の箱を外して箱なしの値を束縛し、箱は遅延の `let` で作る (使われなければ消える)
- [x] `opt.rs`: 単一選択肢の case への `let!` の移動 (FEAT-0066) を、対象が変数でなくても、選択肢が構築子で終わるときに行う (CPR の呼び出しの結果を組み直した `MkBuilder` が既知の構築子になる)
- [x] `opt.rs`: case-of-case で、内側の結果がすべて既知の構築子なら、外側の線形な束縛子も許す
- [x] `reuse.rs`: 対になる確保のない `drop` の再利用トークンを `switch` の腕に渡す (単一の前任者、深さ 3 まで)。確保しない腕ではトークンを解放する
- [x] 単体テスト (`reuse_crosses_switch_arms`、`opt_floats_strict_binding_into_single_alternative_of_call`)

## 受け入れ条件

- [x] 上記のテストがある
- [x] 全ベンチの出力が変わらない (`cargo test --test bench`)、全実行テストが通る
- [x] `scripts/ci.sh` が通る

## メモ

`bench/string-split` (100 万行、最良値):

| 段階 | 確保 | 時間 |
|---|---:|---:|
| 開始時 | 24 M | 0.895 s |
| 静的引数の多相化・`let!` の箱の除去 | | 0.55 s |
| 積型の worker/wrapper・腕をまたぐ再利用 | 5 M | 0.32 s |
| 単一選択肢の case への `let!` の移動 | **2 M** | **0.30 s** |
| Rust | | 0.19 s |

残る 1 行あたり 2 回の確保は `toInt` の `Just` / `I#` で、case の後の計算を腕から箱なしで呼ぶ結合点がないために残る (FEAT-0080)。
