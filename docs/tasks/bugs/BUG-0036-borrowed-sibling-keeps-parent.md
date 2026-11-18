# BUG-0036: 読むだけのフィールドの借用が、マッチしたオブジェクトを生かし、別のフィールドの in-place 更新がコピーになる

- 状態: done
- 重大度: major
- 発見箇所: BUG-0034 (`tests/cirkalang/run/v006/linalg.cirka` の `max-array-copies`)
- 関連: FEAT-0022, FEAT-0257, BUG-0034

## 現象

BUG-0034 の修正の後、`linalg.cirka` の `-O2` で配列のコピーが 1 回増えた (`array_copies` が 9 から 10。上限 9 で失敗)。結果は正しい。

`gemm 1.0 a34 b42 1.0 (withOrder ColMajor c32)` の IO の動作が `case` の腕に入り (BUG-0034)、行列 `c` をマッチした後で
フィールド `xc` (配列) を `bytes_unique` で in-place 更新し、同じ行列のフィールド `oc` (`Order`) をその後の `switch` で読むようになった。
Perceus は `oc` を親から借用する射影として扱い、行列を `oc` の最後の使用まで生かしていたので、
`bytes_unique` の時点で配列を行列のフィールドと `dup` した `xc` の 2 つが指し、コピーになった:

```
alt205:
  let oc%528 = proj res%502 Linalg.Matrix.Linalg.MkMatrix.f2
  let xc%529 = proj res%502 Linalg.Matrix.Linalg.MkMatrix.f3
  dup xc%529
  let p%530 = prim bytes_unique(xc%529)     # RC 2 なのでコピーする
  ...
  switch oc%528 { ... }                     # この後の腕で drop res%502
```

BUG-0034 の前は動作がクロージャになり、`oc` をクロージャに捕らえる (消費する) ので `oc` も射影の直後に `dup` され、行列はマッチの直後に死んでいた。

## 再現手順

`tests/mir/perceus/sibling_fields.mir` の `bump`: オブジェクトをマッチし、配列のフィールドを `bytes_unique` で更新した後に、別のフィールドを `switch` で読む。

```sh
cargo test -p cirka-mir perceus_drops_the_matched_object_before_updating_its_field   # array_copies が 2 (0 であるべき)
```

## 期待される動作

マッチしたオブジェクトがほかに使われないなら、マッチの直後に死に、消費するフィールドの RC は 1 のまま保たれる (in-place 更新がコピーしない)。

## 原因

`cirka-mir::perceus` の dup/drop 挿入は、どこかで消費される射影だけを射影の直後に `dup` し (Koka の順序、FEAT-0022)、
読むだけの射影は親から借用して、親をその最後の使用まで生かす (Lean 4 の borrowed projection)。
読むだけの射影の最後の使用が兄弟の射影の消費より後にあると、消費の時点で親が生きていて、消費するフィールドを親と共有する。

## 修正

消費する射影を消費した直後に、その親 (親が借用した射影なら、借用をたどった先の所有者) がまだ生きているときは、
所有者が所有されていて射影と `switch` 以外に使われなければ、所有者の射影のうちその時点で生きているもの (所有者を生かしているもの) を
射影の直後に `dup` する (`VarInfo::free_sources`。生存区間を計算し直し、変わらなくなるまで繰り返す)。
所有者は残りの借用した射影の最後の使用の後、消費の前に死に、バックエンドの drop 特殊化で、所有者が一意ならフィールドの RC は変わらない。
消費より前に最後に使う射影は従来どおり借用する (`list_map.mir` の `sum` の `head`。`dup` / `drop` を増やさない)。

- [x] 回帰テストを追加 (`tests/mir/perceus/sibling_fields.mir`: 直接のフィールド `bump`、借用した引数 `peek`、2 段のマッチ `bump2`。
  `cirka-mir` の `perceus_drops_the_matched_object_before_updating_its_field`)
- [x] 修正

## メモ

- 最初の修正は、消費する射影を持つ親のほかの射影をすべて `dup` したが、`list_map.mir` の `sum` で `head` の `dup` と `drop` が増えた
  (借用しても親を消費の後まで生かさない)。消費の後の生存で判定するようにした。直接の親だけを見ると、2 段のマッチ (`bump2`) で
  内側のオブジェクトが借用した射影のため対象にならず、コピーが残ったので、借用をたどった先の所有者で判定する。
- `linalg.cirka` の配列のコピーは BUG-0034 と合わせて 9 回 (BUG-0034 の前と同じ)、出力は同じ。
- 全テストの期待出力 (MIR の Perceus のスナップショットを含む) は変わらなかった。ベンチ 22 本 (`bench/*` の CirkaLang・aCHC の版) の
  `-O2` の MIR も、この規則の有無で同一 (Docker で比較)。今の題材では、BUG-0034 で動作が腕に入った `linalg.cirka` の形でだけ効く。
