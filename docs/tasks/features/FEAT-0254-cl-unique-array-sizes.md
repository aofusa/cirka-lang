# FEAT-0254: 一意な配列と文字列のビューの大きさを証明で追う

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P2
- 依存: FEAT-0244
- 設計: [spec-v0.0.5.md](../../design/cirkalang/spec-v0.0.5.md) §4、[16-performance.md](../../design/16-performance.md)

## 概要

FEAT-0244 で共有の配列 (`Array a`) を作る関数の結果に大きさの契約を付け、`A.index` の境界検査を証明で除けるようにした。
一意な配列 (`Unique (Array a)`、種 `LType`) は部分集合型を持てない (spec-v0.0.4.md §3.5) ので、`write`・`read` の境界検査は残る。
数値計算 (v0.0.6) の in-place 更新のループで境界検査を除けるよう、一意な配列の大きさを証明で追う仕組みを入れる。
文字列のビュー (`Data.String`) にも証明済みの読み出しと大きさの契約を付ける。

## 作業内容

- [x] 一意な配列の大きさを証明で追う方法を決める: 大きさを型の添字に持つ `Sized (n : Int) (a : Type) : LType` (構成子は非公開) — spec-v0.0.6.md §5.1、ADR-100
- [x] 証明済みの添字で読み書きする一意な配列の操作 (`readS`・`writeS`・`modifyS`・`swapS`。`Data.Array` と `Data.Array.Unboxed`) と、大きさの契約 (`freezeS` は信頼する仕様、`thawS` は `requires`)。
      unboxed 配列の `index` と `fromList`・`generate`・`map` の大きさの契約も加えた
- [x] `Data.String` のビューの不変条件 (`invariant 0 <= i && i <= j && j <= strLength s`)、証明済みの読み出し `index` とモジュールの繰り返しの検査の除去、
      `toStr`・`take`・`drop`・`slice` の結果の長さの契約 — spec-v0.0.6.md §5.2
- [x] 検証器: 構成子の不変条件を分解していない値の射影にも仮定する。展開する定義に渡された、結果に部分集合型を持つ呼び出しを呼び出しのまま扱う (§5.3)
- [x] 効くベンチを加えて計測する (16-performance.md §4): `bench/stencil` (一意な配列の in-place 更新の繰り返し)、`bench/string-split` (文字列のビュー)
- [x] 計測で見つけた後退の修正: Core の簡約で、呼び出しを返すだけの `let! b = f a in b` を `f a` にする (06-ir.md §1.3)

## 受け入れ条件

- [x] 一意な配列の 0 から n - 1 までを書く繰り返しの `write` が、境界検査なしで示される (`tests/cirkalang/run/v006/sized_arrays.cirka` の `squares`。
      `cirkalang_sized_arrays_remove_checks` が MIR に検査のある演算がないことを確かめる)
- [x] 効果をベンチで計測しチケットに記録する (下のメモ。bench-report.md の「一意な配列の大きさと文字列のビューの証明 (FEAT-0254)」)
- [x] テストを追加し `scripts/ci.sh` が通る (Docker、Z3・Vulkan・OpenBLAS・TSan の構成を含む)

## メモ

- FEAT-0244 の計測: 共有の箱入り配列の内部の繰り返しを `index` で読んでも速くならなかった (LLVM が繰り返しの条件から検査を消し、
  残りは箱の確保が支配的)。効くのは、LLVM が条件から示せない添字 (別の配列の大きさとの関係、クロージャを経る添字) と、
  箱のない要素の繰り返し (ベクトル化の妨げになる検査) と見込む。
- 作業中に見つけたバグ: BUG-0029 (検査器が配列を作る繰り返しを評価して指数時間。`sized_arrays.cirka` の検査が 76 秒・4.8 GB)、
  BUG-0030 (型が後で宣言する関数を使うと診断なしに誤りの項になる。E1121 を加えた)、BUG-0031 (同じ呼び出しが深さによって別の項に訳され、
  `bench/stencil` の `U.thawS n (U.generate n f)` が示せなかった)。
- `Int` は 64 ビットでラップするので (BUG-0018)、`Data.String` の添字の計算は溢れない形に書き直した (`i + clamp n 0 (j - i)`、`decode` の続きのバイトはオフセットで読む)。
  文字列の長さに上限 (例えば 2^62 未満) を仮定すれば書き直しは要らないが、止まらない呼び出しの `let` が最適化で消えると、その上限と信頼する仕様
  (`generate n f` の大きさ `n`) から矛盾が導けて健全でなくなるので入れなかった。
- テスト: `run/v006/sized_arrays.cirka` (`-- max-array-copies: 1`)・`run/v006/string_views.cirka` (どちらも Z3 が要る)、`verify/fail/sized_off_by_one.cirka`・
  `verify/fail/string_index.cirka`、`verify/pass/record_views.cirka` (検証器の二つの変更。HEAD の検証器では二つとも示せないことを確かめた)、
  `verify/pass/parsed_sizes.cirka` (BUG-0031)、`fail/refine/used_before_signature.cirka` (BUG-0030)、`cirkalang_sized_arrays_remove_checks`・`cirkalang_string_views_remove_checks`
  (MIR に検査のある演算が残らない。`--contracts=off` では残る)、`cirkalang-elab` の `stuck_primitives_do_not_evaluate_their_arguments` (BUG-0029)。
- 計測 (Docker、hyperfine 15 回、callgrind):
  - `bench/stencil` (100 万要素の 3 点ステンシルを 100 回): 証明あり 215.1 ± 13.9 ms、`--contracts=off` 226.2 ± 8.6 ms、Rust 215.5 ± 10.8 ms。
    命令数 (20 万要素) は 99.7 M / 124.7 M / 98.5 M、条件分岐は 5.26 M / 10.26 M / 5.20 M。`--contracts=off` ではループの入口の検査
    (FEAT-0069) が `i` と `i + 1` の検査を除くが、`i - 1` の読み出しの検査が残る。証明は 4 つの読み書きすべての検査を除き、Rust と同等になった。
  - `bench/string-split` (100 万行): 証明あり 261.4 ± 6.2 ms、`--contracts=off` 270.8 ± 8.6 ms、FEAT-0254 の前のライブラリ 270.8 ± 7.8 ms、
    Rust 182.1 ± 4.2 ms (1.44 倍)。命令数 (10 万行) は 249.2 M / 258.7 M / 258.7 M / 161.0 M (3.7% 減)。
  - 計測で見つけた後退: `ensures` を加えた `skipSpace` などは自己呼び出しの結果を `primRefine` に通すので Core に `let! r = f x in r` が残り、
    末尾呼び出しでなくなり CPR も効かず (戻り値の `I#` を毎回確保)、string-split が 2.2 倍遅くなった (591 ms)。呼び出しの結果は WHNF なので
    `let! b = f a in b` は `f a` と同じで、Core の簡約に加えた (aCHC にも効く。共通基盤のテスト `opt_strict_let_of_returned_call_is_the_call`)。
- 効果の範囲: 時間の差は小さい (ループの入口の検査と LLVM が多くの検査を除き、残る検査の分岐はよく当たる)。証明の利点は、最適化の
  見込みに頼らず検査がないことが型で決まることと、入口の検査が示せない添字 (`i - 1`、両端からの添字、別の配列の大きさとの関係) に効くこと。
