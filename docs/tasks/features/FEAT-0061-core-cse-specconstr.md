# FEAT-0061: 共通部分式除去・ループ不変式の移動・SpecConstr とゼロコストの回帰テスト

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0041
- 設計: [16-performance.md](../../design/16-performance.md) §2・§4

## 概要

純粋性から自由にできる Core の最適化のうち未実装のものを加える。あわせて、抽象版と手書き版の生成物が一致すること
(ゼロコスト抽象化) を確かめる回帰テストの仕組みをハーネスに入れる。

## 作業内容

- [x] 共通部分式除去: 調査して見送り (メモ。16-performance.md §4.1、ADR-083)
- [x] ループ (再帰関数) の不変式の移動: 調査して見送り (同上)
- [x] 静的な関数引数での特殊化 (`sat.rs`、FEAT-0059 の調査から追加): 再帰の間で変わらず呼ばれるだけの関数引数に、呼び出し側の既知の関数を入れた写しを作る。
      大域関数を原子に部分適用したもの (`a i`) も既知の関数として扱う
- [x] 構築子による特殊化 (SpecConstr): 調査して見送り (同上)
- [x] ハーネス: 抽象版と手書き版の組で確保回数・実行した命令数・RC 操作の数・最適化後の MIR の大きさを比べるヘッダ `-- same-cost-as: x.hs [+N]`
      (aCHC と CirkaLang の run テスト。インタプリタが実行した命令を数える `Stats::steps`)
- [x] ハーネスが見つけた差の修正: newtype を通した worker/wrapper、別名の大域定義の値の位置での展開、使われない引数の除去 (借用のトークン)
- [x] ベンチで前後を計測する

## 受け入れ条件

- [x] 各最適化のユニットテストと、ゼロコストの組のテスト (newtype・型クラス・高階関数・`Borrow`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ
- 静的な関数引数での特殊化 (2026-11-06): 対象は単相で再帰する大域関数のうち、関数型の引数が (1) すべての再帰呼び出しで同じ位置にそのまま渡され、
  (2) それ以外では呼び出しの頭にしか現れないもの。呼び出し側がその位置に大域関数か λ を渡すと、λ の自由変数を先頭の引数にした写し `f$fN` を作り、
  再帰呼び出しも写しへ向ける。簡約器が λ をその場で適用し、箱が消える。写しは呼び出しの形 (自由変数を番号に置き換えた式) ごとに 1 つ、全体で 200 まで。
  spectral-norm `own.chs` は 1.44 s → 0.177 s (Rust 0.175 s)、確保は n=100 で 612,842 → 601。テスト: `opt_specializes_static_function_arguments`、
  `opt_keeps_function_arguments_that_change_or_escape`、`tests/achc/run/h3/static_fun_arg.hs` (100 万回の反復 2 つで確保 115 回)。
- ゼロコストの組のハーネス (2026-11-14): `-- same-cost-as: FILE [+N]` を付けた run テストは、同じディレクトリの手書き版 `FILE` と -O2 で比べ、
  確保回数・実行した MIR の命令数 (`interp::Stats::steps`、命令と終端命令)・RC 操作の数・最適化後の MIR の大きさ (命令と終端命令の数) が
  手書き版を超えないことを確かめる。組は `tests/achc/run/zerocost/` の newtype・型クラス・高階関数 (演算子・λ・部分適用) と
  `tests/cirkalang/run/v003/borrow_loop.cirka` (借用。`cirkalang_borrow_costs_nothing` を置き換え)。
- ハーネスを入れた時点の結果 (n = 100,000): 型クラスの組は一致。newtype の組は命令数 1,100,478 対 500,473、RC 操作 100,082 対 81
  (累積引数 `Meters` の箱が消えない: worker/wrapper の箱型の判定が newtype を通らなかった)。高階関数の組は確保 200,076 対 74
  (`foldLoop (+)` の `(+)` が `$cNum$+ = primIntAdd` の別名で、別名が呼び出しの頭でしか展開されず、静的な関数引数の特殊化が既知の関数と見なさなかった)。
  借用の組は `Ref` の単位値 (CAF) をループの引数に渡し続けていた。
- 修正: (1) worker/wrapper が箱型の newtype (入れ子も可、線形を除く) の引数と結果を `cast` を通して unboxing する (`ww::box_via`)。
  (2) 別名 (型抽象・型適用・`cast` の下が大域変数だけの定義) を値の位置でも展開する。(3) 自分の再帰呼び出しの同じ位置へ渡すだけ (か使わない)
  引数を worker が受け取らない (`ww::absent`。状態トークンと unboxed の引数は残し、すべてが当たるときは最後の 1 つを残す。線形な引数を持つ関数も対象)。
  (4) 静的な関数引数の特殊化で、大域関数の原子への部分適用 (`addScaled 2`) を既知の関数とする。
  修正後、newtype と借用の組は MIR が名前と引数の順以外で手書き版と同一、高階関数の組は同じ関数の集合 (並びだけ違う)。
- テスト: `opt_worker_wrapper_unboxes_newtypes`、`opt_worker_wrapper_drops_absent_parameters`、`opt_specializes_aliases_and_partial_applications`、
  `opt_keeps_polymorphic_aliases_in_dictionaries`、`interp_counts_executed_instructions`、ゼロコストの組 4 つ。
- CI (デバッグビルドの Core Lint) で見つかった誤り: 多相な別名 (辞書のメソッド `$fFunctor$f0 = Λa b. fmapList @a @b`) を型適用のない値の位置で
  展開すると、辞書の構築子のフィールドに `Λ` が残って原子でなくなり、特殊化の後の辞書フィールドの大域化が同じ名前 `$f0` をもう一度定義していた
  (release ではリントが走らないのでホストの統合テストは通っていた)。多相な別名は、型抽象の数以上の型適用の下でだけ展開するようにした
  (回帰テスト `opt_keeps_polymorphic_aliases_in_dictionaries`)。
- ベンチの前後 (変更前のコンパイラと全ベンチの LLVM IR を比較、2026-11-14): IR が変わったのは fannkuch-redux `plain.hs`・spectral-norm `plain.hs`
  (別名の展開)・n-body `main.cirka` (借用のトークンの除去) の 3 つだけ。交互に 10 回ずつ実行した最小の CPU 時間 (ホストの負荷 3〜7):
  spectral-norm `plain.hs` (1000) 0.876 → 0.892 s、fannkuch-redux `plain.hs` (9) 2.900 → 2.879 s、n-body `main.cirka` (5,000,000) 0.575 → 0.584 s で、
  いずれも誤差の範囲 (±2%)。出力は同一。array-sort `plain.hs` は aCHC では作れない (Control.Monad.ST)、binary-search は Z3 が要るので対象外。
- 共通部分式除去・ループ不変式の移動・SpecConstr は見送り (16-performance.md §4.1、ADR-083): 熱いループの重複と不変式は unboxed の算術と
  配列の読み出しで LLVM -O3 が消している (n-body `main.cirka` の `pairs` は MIR で `i * 7`・`j * 7` を 7 回ずつ計算するが、-O3 の後は各 1 回)。
  Core に残るのは spectral-norm `plain.hs` の `dot` のループで正格でない静的引数 `i` を反復ごとに `force` する 3 命令 (約 25 命令中) だけで、
  費用の大半は共有リストをたどる RC 操作。自己呼び出しに構築子を渡すループはベンチにない。
- 調査で見つけた別の改善点: n-body `plain.hs` の `interactAll` は 1 組ごとに 3 つのサンク (12 個の自由変数) を作る (`pairStep` の組の要素が遅延)。
  要素は安価で失敗しない計算 (浮動小数点の算術と `sqrt`) なので、先に評価すればサンクが消える → FEAT-0083。
