# FEAT-0091: Rust を上回る性能 (Rust より遅い題材の原因の調査と改善)

- 状態: blocked
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0263
- 設計: [16-performance.md](../../design/16-performance.md), [bench-report.md](../../design/bench-report.md) の「CirkaLang v0.0.7」

## 概要

CirkaLang v0.0.7 のリリースのベンチで、binary-search 1.45・n-body 1.44・cli-grep 1.49・string-split 1.39・session 1.15・
HTTP サーバ 1.29 (16 接続) が Rust より遅く、array-sort・mandelbrot・spectral-norm・stencil・effects は同等 (0.97〜1.01) だった。
性能の方針 (16-performance.md §1: 型・純粋性・一意性・証明から分かる事実を使って Rust を上回る) に従い、題材ごとに差の原因を計測で確かめ、
一般の最適化 (題材ごとの特別扱いをしない) で改善して、すべての題材で Rust を上回ることを目指す。

## 作業内容

- [x] 題材ごとに差の原因を計測で確かめる (callgrind / cachegrind、生成した LLVM IR とアセンブリの比較、LLVM IR を手で直した実験)
- [x] 改善を個別のチケットにして実装し、効果を計測して記録する
- [x] すべての題材を計測し直し、bench-report.md に記録する

## 受け入れ条件

- [ ] すべての題材 (11 題材と HTTP サーバ) で CirkaLang が Rust 以上の速さ (同じ条件で交互に実行した中央値の比が 1.00 以下)
- [x] 改善の前後で全ベンチの出力が変わらない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- binary-search (1.45): 要素が箱の `Array Int` を読むたびに、要素の `force` (サンクかの検査) と、その前の `dup`・後の `drop` が入る
  (MIR: `array_get_unchecked` → `dup` → `force` → `switch` → `drop`)。内側の反復は約 25 命令 (Rust は約 11 命令。どちらも分岐で、
  予測の外れが多い)。共通の MIR への変換は、要素を遅延のまま格納し、読み出しで `force` する (aCHC の配列の意味論)。CirkaLang の値は
  サンクでない (`Lazy a` も構成子 `Lazy#` の値) ので、要素を評価してから格納すれば、読み出しの `force` は要らず、借用した射影のまま
  `dup` / `drop` も要らない。また、構成子が 1 つの型の `switch` の既定の腕が `panic` なので、LLVM がタグの検査を消せない。
- n-body (1.44): Rust は 5 体の 2 重ループを定数の繰り返し回数で完全に展開する (50 万ステップで分岐 56 万回)。CirkaLang の
  `pairs` / `moves` は (i, j) を持つ末尾再帰のループで、分岐 1,804 万回 (1 ステップ 36 回)・予測の外れ 100 万回 (cachegrind の模擬)、
  命令数は Rust の 1.26 倍 (2.88 億と 2.28 億)。配列の大きさが実行時の値なので、要素の読み出しごとの境界検査も残る。
  定数の引数での呼び出しを特殊化してループを展開すれば、添字が定数になり、検査もループの外に出せる。
- cli-grep (1.49)・string-split (1.39): バイトの検索は共通 Core の 1 バイトずつのループ (`cirka$bytes_find_byte_loop`)。
  LLVM 18 は途中で抜けるループをベクトル化しない。確保は 100 万行で 100 万回 (cli-grep)、200 万回 (string-split)。調査中。
- session (1.15)・HTTP サーバ (1.29): 調査中。
- 2026-11-25 の状況 (FEAT-0097〜0099 の後、i5-3320M、11 回の交互実行の中央値、Rust に対する比): array-sort 1.06・binary-search 1.18・
  cli-grep 1.02・mandelbrot 0.98・n-body 1.16・spectral-norm 1.01・stencil 1.01・string-split 1.06・effects 1.00・session 0.23・
  binary-trees 0.42 (別のプロジェクトのビルドと重なり Rust 側が遅れた。測り直す)。
- n-body・LLVM の版: Rust 1.99.1 は LLVM 23.1、こちらは clang 18 だった。生成した IR を clang-23 でコンパイルし直すと (使い捨ての
  コンテナに apt.llvm.org の clang-23 を入れた実験)、n-body 0.86 倍 (Rust の 0.97)・effects 0.93 倍 (0.95)・stencil 0.98 倍 (0.99)、
  array-sort 1.03 倍 (1.08)・binary-search 1.03 倍 (1.22)・string-split 1.02 倍 (1.07)、他は ±0.5 % (出力はすべて同じ)。
  バックエンドを LLVM 23 に上げる (FEAT-1000、ADR-110)。
- binary-search: 題材を `Data.Array` (要素が箱の `Array Int`) から `Data.Array.Unboxed` (`UArray Int`) に変えた。Rust の `Vec<i64>` と
  同じく要素を直接並べる配列で、アルゴリズム・入力・出力は同じ。`index` は同じく契約で範囲内と証明され、検査がない。箱の版は探索の
  1 回ごとに要素の箱を辿る (ポインタを読んでから値を読む) ので Rust の 1.18 倍 (LLVM 18)・1.22 倍 (LLVM 23) だった。`UArray Int` の版は
  0.80 倍 (LLVM 18、15 回の交互実行の中央値)。`(hi - lo) / 2` を論理シフトにする実験 (符号付きの割り算の補正の 2 命令を省く) は、
  unboxed の版では分岐のない形 (`cmov`) が崩れて 0.80 → 1.12 倍と遅くなったので採らない。
- 2026-11-26 の状況 (FEAT-1000〜1031 の後、Docker。FEAT-1031 までのコンパイラのプログラム・FEAT-1017 の版・Rust を交互に 15 回
  (binary-trees は 5 回) 実行した中央値。`docs/artifacts/f1027/abfinal.sh`): Rust に対する比は array-sort 0.917・binary-search 0.787・
  binary-trees 0.292・cli-grep 0.760・mandelbrot 0.985・n-body 0.964・spectral-norm 0.709・stencil 0.949・string-split 0.696・
  effects 0.930・session 0.220。11 題材すべてで Rust より速い (出力はすべて同じ)。計測の間、CPU は負荷をかけても 1.2 GHz のままだった
  (定格 2.6 GHz、ターボ 3.3 GHz。温度による抑制の記録はなく、電源の側の制限とみられる)。比は同じ条件で比べたもの。
- HTTP サーバ (20 万要求、`docs/artifacts/f1027/httprot.sh`。CirkaLang と Rust の順を回ごとに入れ替えて 21 回ずつ): Rust / CirkaLang の
  スループットの比の中央値は 16 接続 0.995・64 接続 1.005 (Rust が先の回 1.015 / 1.004、CirkaLang が先の回 0.973 / 1.006)。
  16 接続は揺らぎが大きく (カーネルの時間が 38.6 µs と他の計測の 32〜34 µs より長く、回ごとの比が 0.75〜1.23)、先に走った方が約 2 % 速い。
  それまでの順を固定した計測 (Rust がいつも最後) では 1.009 / 1.009 (15 回)、0.996 / 0.999、0.996 / 0.999 (9 回ずつ) で、
  組ごとに ±1 % 動く。Rust と同等 (比 1.00 ± 0.01) で、どちらとも言えない。
- HTTP サーバの時間の内訳: 1 要求あたり `recv` 1 回・`send` 1 回と `epoll_wait` の一部 (callgrind の `syscall` 命令の実行回数) で、
  Rust のサーバ (イベントごとに `read` 1 回・`write` 1 回) と同じ数のシステムコール。CPU 時間はカーネル 32〜34 µs (Rust より 0.2〜0.5 µs 短い。`recv` / `send` は `read` / `write`
  のファイルの検査を通らない、FEAT-1013) とユーザ空間 3.75〜3.80 µs (Rust 3.35〜3.45 µs)。カーネルが約 90 % で、ユーザ空間の差 (約 0.4 µs)
  は要求の時間の約 1 %。
- ユーザ空間の差は、システムコールの後に触れるキャッシュラインの数による (コード 66〜68 本・データ 26〜28 本、Rust はコード 42 本・データ 23 本。
  lackey)。1 要求で実行するコードは 2.5 KB (詰めれば 39 本) で、残りは熱いブロックの間に挟まった冷たいブロック (関数の末尾へ送られた
  `drop` の解放の経路、静的なオブジェクトの検査、`out ++ hello` の空でない側など) が埋める部分的なライン (`answer` 21 本のうち 8.4 本、
  送信の継続 9 本のうち 4.4 本ほか、計約 27 本)。プロファイルなしで配置を変える実験はどれもラインを減らさなかった: `drop` の
  `rc > 1` の重みをなくす (67.8 本 (元 68.0 本)、binary-trees の命令数 +1.4 %)・逆にする (73.5 本)・LLVM の ext-TSP の配置 (67.0 本。
  重みなしと合わせて 71.0 本)。ランタイムの LTO (FEAT-0060 のメモ) は命令数を 1.5 % (インライン化の閾値 1000 で 7 %) 減らすが、
  ラインが増え (67.3 / 69.5 本)、スループットは変わらない。
- 残る手段はシステムコールを Rust より減らすこと (io_uring で複数の接続の受信と送信をまとめる) だが、コンテナの seccomp の設定
  (`docker/seccomp.json`、Docker の既定) が io_uring を禁じる。HTTP サーバの受け入れ条件 (比 1.00 以下) はこの条件では確かに満たせないので、
  `blocked` とする (io_uring を許すかはコンテナのセキュリティの設定を変えるので利用者が判断する。許さない場合は HTTP サーバの条件を
  「同等 (±1 %)」に改めて閉じる)。11 題材の条件・出力・テストは満たしている。
