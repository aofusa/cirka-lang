# FEAT-0090: 線形な値を返す case の case-of-case

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0262
- 設計: [06-ir.md](../../design/06-ir.md), [16-performance.md](../../design/16-performance.md)

## 概要

分岐ごとに構成子を作って返す式をすぐに `case` で分解するとき、共通 Core の case-of-case は外側の `case` を内側の各分岐に入れ、
構成子を作らずに済ませる (FEAT-0041)。次の場合はこれが起きず、構成子が確保される:

- 内側の分岐の結果が `let t = K .. in K2 t` の形のとき。結果の形が分からず、外側の選択肢が大きいと case-of-case をしない。
- 内側の `case` と外側の `case` の間に、自分の型への `cast` があるとき (`IO1` の束縛の適用が λ のキャストを通り抜けた後に残る)。
  `let! b = case .. in case b of ..` の連結 (FEAT-0074) が働かない。

`Std.Session` の `offer` (spec-v0.0.7.md §3) は `if b then Left (MkChannel o i) else Right (MkChannel o i)` を返し、
呼び出し側がすぐに `case` で分解するが、`bench/session/main.cirka` では受け取る側が 1 往復ごとに `MkChannel` と `Branch` の 2 つを確保する
(`direct.cirka` は 1 往復 2 つ、`main.cirka` は 4 つ)。

## 作業内容

- [x] 内側の結果が `let` / `let!` の下の構成子でも、結果の形が分かるものとして case-of-case をする (各写しは既知の構成子の `case` としてすぐに簡約される)
- [x] 変数の自分の型への `cast` を除く
- [x] 単体テスト (共通 Core) と Core の実行テスト、`bench/session` の確保の数・命令数の比較

## 受け入れ条件

- [x] `bench/session/main.cirka` の確保の数が `direct.cirka` と同じになる
- [x] ほかのベンチが遅くならない
- [x] テストを追加し `scripts/ci.sh` が通る

## 結果

- 確保 (10 万往復): `main.cirka` 400 010 → 200 008 (`direct.cirka` 200 009)。命令数 (callgrind) 312.9 M → 256.7 M (`direct.cirka` 252.4 M)。
  時間は変わらない (交互に 15 回の中央値で 1.207 s → 1.209 s。スレッドを起こすカーネルの時間が大半)。
- ほかのベンチ: 生成した LLVM IR が変わったのは cli-grep と HTTP サーバだけ (Docker で全ベンチを前後のコンパイラで作って比べた)。
  - cli-grep: 命令数は同じ (2034.4 M)、時間は交互に 15 回の中央値で 322 ms → 325 ms (揺らぎの範囲)。変わったのは自分の型への `cast` と、エラーの経路の形だけ。
  - HTTP サーバ: 要求ごとの `Just` / `Ok` / `Err` などがすぐに分解されて作られなくなり、1 接続 2 万要求の命令数 (callgrind) は
    71.9 M → 51.8 M (-28%)、`malloc` の命令数は 7.96 M → 4.74 M。スループット (16 接続、交互に 3 回) は 45.5k → 45.1k 要求/s で、
    同じ版の中の揺らぎ (41k〜53k) を超える差はない。
- 自分の型への `cast` がなくなったことで、状態トークンの worker の結果が CPR で unboxed になる場合がある (単体テスト `opt_gives_io_functions_the_state_token`
  の `loop$io` は `Int#` を返す `loop$io$c` になった)。
- 計測のスクリプトとログは `docs/artifacts/f0090/` (git 管理外)。

## メモ

- 最初は外側の `case` の束縛子 (分解する値) が線形なことも原因と考えたが、CirkaLang の Core の束縛子はすべて `Many`
  (線形性はフロントエンドが検査する) で、`bench/session` の妨げではなかった。線形な束縛子での case-of-case (aCHC の `.chs` や手書きの Core) は、
  写しの中で既知の構成子に束縛された変数の `case` を簡約する規則 (`known_scrut`) が線形な変数を簡約しないので、効果を得るには
  そちらも変える必要がある。必要になるまで入れない (`case_of_case_ok` は線形な束縛子を今までどおり断る)。
