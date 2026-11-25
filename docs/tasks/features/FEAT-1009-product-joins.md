# FEAT-1009: 分岐が作ってすぐ分解する積の値は、フィールドを合流点へ渡す (積の合流点)

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0065, FEAT-0074, FEAT-0091
- 設計: [06-ir.md](../../design/06-ir.md) §1.7, [00-overview.md](../../design/00-overview.md) ADR-114

## 概要

bench/string-split と bench/cli-grep は (FEAT-1008・FEAT-0082 の後も) 1 行に 4 回確保する。どちらも `Data.String.Builder` への
`append` ごとに `MkBuilder` を 1 つ確保する。`append` は `reserve` をインライン化し、`if n + k <= cap then MkBuilder buf n cap
else grow …` の結果をすぐ `case` で分解する。伸長の分岐は CPR の worker の呼び出し (`case grow$w$c … of UTup3 … -> MkBuilder …`)
なので「すべての分岐が構築子」の case-of-case (FEAT-0090) に当たらず、続き (書き込みと次の `append`) は大きいので複製もしない。
Core には合流点がないので、分岐の値は箱で運ぶしかない。

積の合流点: 構築子 1 つ・フィールド 2 つ以上の型の値を分岐が作ってすぐ `case` が分解するとき、分岐の返却を箱なしの組 (`UTup<n>`、
FEAT-0065) にして、`case` はその組を分解する。MIR は分岐を **合流ブロック** (引数 = フィールド) へのジャンプに下げる (値の `case` が
1 つの値で使う合流ブロックを複数の値に広げる)。何も確保せず、続きも 1 つのまま。発散を除くすべての返却が構築子のときだけ行い
(既にある値を返す分岐の箱は、続きが再利用できるので分解しない)、正格な箱のフィールドは構築子が評価した位置で評価する。

## 作業内容

- [x] 設計: 06-ir.md §1.7、ADR-114
- [x] `cirka-core` の `join.rs`: 積の合流点の書き換え (最適化の最後の worker/wrapper の後)。Core Lint の箱なしの組の置き場所
  (分岐の返却)。case-of-case は箱なしの組を分解する選択肢を小さいときだけ複製する
- [x] `cirka-mir` の下げ: 箱なしの組を分解する `case` の被検査式が分岐なら、合流ブロックへのジャンプにする (ポインタの引数は
  すべての分岐で評価済みのときだけ評価済み)
- [x] テスト: 合流点の書き換え (正格な箱のフィールド・発散)、値として使う積と既にある値を返す分岐は対象外、
  Core Lint が分岐の返す組を受け付けること、手書きの合流点の実行 (`tests/core/run/utup_join.core`)、CirkaLang の実行
  (`v007/product_join`、`v007/fold_accumulator` の確保の上限)
- [x] 計測: string-split・cli-grep の確保と時間、全ベンチ

## 受け入れ条件

- [x] `Builder` への `append` を重ねるループが反復ごとに確保しない (`v007/fold_accumulator` の確保が行数によらない)
- [x] bench/string-split と bench/cli-grep が速くなり、ほかのベンチが遅くならない (出力は同じ)
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ

- 2026-11-25: 確保の回数 (ネイティブ、`CIRKA_STATS`)。bench/string-split (引数 100000) 400036 → 36、bench/cli-grep (100000)
  400062 → 62 (どちらも行数によらない。出力は同じ)。`v007/fold_accumulator` は 2014 → 14 (上限を 2100 から 40 に下げた)、
  `v007/product_join` は 2。
- case-of-case が箱なしの組を分解する小さい選択肢を分岐へ複製すると、複製の `case` の束縛子は出現の情報をまだ持たないので
  「使われる」と見なされ、構築子が既知の `case` が組を `let` で束縛して Core Lint に落ちた (`tests/core/run/utup_join.core` の最適化)。
  組の束縛子は Lint により使われないので、既知の構築子の規則は組の束縛子を束縛しない (`opt_case_of_case_on_tuples_binds_no_tuple`)。
- 最初の版は、変数や呼び出しの結果を返す分岐もその場で分解して合流させていた。CI の `v007/io_after_case` で確保が 1 反復 1 回
  増えた (上限 100 に対し 1076): `withOrder` の分岐の一方が引数 `b` をそのまま返し、続きの `bumpIO` が作る `MkBuf` は `b` の箱を
  再利用していた (Perceus) が、合流点では `b` を分岐で分解して解放し、続きで新しく確保する。発散を除くすべての返却が構築子の
  ときだけ行うことにした (分岐の箱はどれも新しく作るものなので、続きの再利用は確保が減る分で相殺され、確保は増えない)。
- 2026-11-25: 全ベンチ (FEAT-1008 のビルドに対して、LLVM 23、15 回の交互実行の中央値、binary-trees は 5 回、別の作業の
  ビルドと重なり負荷 2.8〜5.3)。出力はすべて同じ。IR が変わったのは cli-grep と string-split だけで、cli-grep 0.853 倍
  (Rust の 0.846 倍)、string-split 0.829 倍 (最小値の比 0.838。Rust の 0.678 倍だが Rust 側が負荷で遅れた。最小値の比 0.755)。
  HTTP サーバの 1 要求の命令数は 1,406 → 1,352 (Rust 669)。
