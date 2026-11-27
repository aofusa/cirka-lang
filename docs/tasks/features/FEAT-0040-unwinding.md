# FEAT-0040: 例外/パニックのアンワインド

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0031
- 設計: [05-runtime.md](../../design/05-runtime.md) §6, [06-ir.md](../../design/06-ir.md) §1.8・§2.5, [07-backend.md](../../design/07-backend.md) §2, ADR-123〜128

## 概要

Itanium ABI のアンワインドと landing pad での drop。共通 Core・MIR・LLVM・ランタイムに例外の投げ方 (`raise`) と捕まえ方 (`catch`) を入れ、
例外の経路でも所有の参照を解放する (リーク 0)。`catch` を含まないプログラムは今と同じコード (中止の様式、ADR-125) で、性能は変わらない。
aCHC の `Control.Exception` (`catch`/`bracket`/`finally`/`throwIO`) と W0305 は FEAT-0145、CirkaLang の panic のアンワインドと
`Drop`/`Release` の実行は v0.1.0 の FEAT-0302 で、この共通基盤の上に作る。

## 作業内容

- [x] MIR: 終端命令 `raise x`、`let (st, v) = catch apply c(args)`、テキスト形式、検証器、様式 (`Module::catches`)
- [x] MIR: 投げうる関数 (`unwind::may_raise`) と後始末の集合 (`unwind::cleanups`、最終の MIR の所有の数)
- [x] MIR インタプリタ: `Exit::Raise`、後始末、`catch apply`、例外を覚えるサンク、捕まらない例外の表示と `RunResult::orderly`
- [x] ランタイム: パーソナリティ関数、例外の枠と保留の枠、`cirka_exception_*`・`cirka_uncaught*`、様式の印、例外を覚えるサンク、
      `<<loop>>`・`cirka_panic_bytes`・ファイルの誤り・バイト列の範囲外を保留の枠で返す
- [x] コード生成: `nounwind` の除去、`invoke` と landing pad、冷たいブロックでの後始末と投げ直し、`catch apply`、入口 (`cirka_main`・
      サンクのスタブ・`cirka_call_1`・C のコールバック)、`apply` の汎用の関数、24 バイトのサンク
- [x] Core: プリミティブ `raise`・`catch`、Core Lint、簡約器・正格性解析での扱い、MIR への変換
- [x] テスト: MIR の実行テスト (捕まえる・捕まらない・後始末・サンク・CAF、インタプリタとネイティブの一致)、Core の実行テスト
      (`catch` で作った解放処理が例外の経路で動くことを出力で確認)、コード生成の単体テスト、ランタイムの単体テスト
- [x] 性能: 全ベンチで中止の様式の生成コードが変わらないことを確かめ、アンワインドの様式の費用を計測する
      (`--unwind`: `catch` のないプログラムをアンワインドの様式にする。計測と試験のため)
- [x] レビューで見つけた不具合の修正 (CAF の例外の共有、`catch` の Lint、`catch` の前へ評価を動かさない正格性解析 (ADR-128)、BUG-0046)

## 受け入れ条件

- [x] 例外発生時に解放処理が実行されることを出力で確認 (Core の `catch` で作った解放処理)
- [x] 例外経路でもリーク 0 (捕まえた例外・捕まらなかった例外の両方、インタプリタとネイティブの確保・解放の回数が一致)
- [x] `catch` を含まないプログラムの生成コードと性能が変わらない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 試作 (docs/artifacts/f0040/exp1): Rust (no_std) のパーソナリティ関数と LLVM の `invoke`/`landingpad`/`resume` で、`tailcc` と `musttail` の
  フレームを越えてアンワインドできる。clang のリンクは libgcc_s を自動で付ける。panic=abort の Rust の `extern "C-unwind"` の関数は
  アンワインドを中止する landing pad (`rust_eh_personality`) を持つので、Rust のフレームはアンワインドしない (ADR-123)。
- W0305 (例外の経路で漏れうる線形の値) は aCHC の `.chs` の規則なので FEAT-0145 に移した。
- 生成コードの比較: ベンチのプログラム 27 個 (MIR・Perceus 前の MIR・CirkaLang・aCHC) の LLVM IR を、変更前 (a764c85) と
  変更後のコンパイラで出力して比べた。26 個は、使わない `cirka_raise_abort` の宣言が増えるほかは同じ。array-sort の `plain.hs` は
  どちらのコンパイラでもコンパイルできない (aCHC に `Data.Array.ST` がない)。レビューの修正の後にもう一度比べ、同じ結果だった。
  生成コードが同じなので、`catch` を含まないプログラムの性能は変わらない。
- アンワインドの様式の費用 (`--unwind` で、`catch` のないベンチ 16 個をアンワインドの様式にした):
  - 実行ファイルは CirkaLang で 1.3% (約 14 KB、session は 0.9%)、aCHC の `plain.hs` で 4.6〜6.1% 大きくなる (後始末の表と landing pad)。
  - 時間 (hyperfine 10 回) は 11 個で差が −4.1〜+1.4% (binary-trees `main.cirka` はアンワインドの様式が 4.1% 速い)。
    ばらつきの大きかった 5 個は、命令数 (cachegrind、小さな入力) と、
    2 つの版の順番を入れ替える 6 組 × 5 回 (中央値) で測り直した:

    | プログラム | 命令数 | 時間 (中央値) |
    |---|---|---|
    | spectral-norm `plain.hs` | +4.6% (207.5 M → 217.2 M、n=200) | +5.8% (1.306 s → 1.381 s) |
    | fannkuch-redux `plain.hs` | +2.0% (1574 M → 1605 M、n=8) | +2.9% (6.046 s → 6.219 s) |
    | cli-grep | +1.6% (84.0 M → 85.3 M) | +3.2% (0.345 s → 0.356 s) |
    | effects | +0.02% | 同じ (1.356 s) |
    | session | スレッドの順序で ±0.5% 揺れる (差は揺れの範囲) | 同じ (0.454 s → 0.453 s) |

  - 増えた命令の場所: spectral-norm はすべてサンクのコード (`cirkaFoldrList` の 4 つの `$thk`)。アンワインドの様式のサンクの
    スタブは、例外を止めて保留の枠に置くために本体を `invoke` で呼ぶので、本体への末尾呼び出しにならずフレームを作る。
    fannkuch-redux はサンクのコードと `++`・`take`・`drop`・`zip` などに分散 (投げうる呼び出しをまたいで参照を持つ `invoke`)、
    cli-grep は `main` と `appendInt` (投げうる extern の呼び出しをまたぐループ)。減らす工夫は FEAT-1032 に起票した。
- レビュー (変更全体) で見つけて直した不具合:
  - ランタイム: 単一スレッドのうちに例外を投げた CAF の例外を、プロセスが MT になるときに共有マークしていなかった
    (`thread::share_caf`、05-runtime.md §6.4・§9.1)。ほかのスレッドが CAF を評価して例外を dup するのと、例外を捕まえたスレッドが
    drop するのとが、原子的でない参照カウントの更新で競合する。ランタイムの単体テスト `a_raised_caf_shares_its_exception` と
    `tests/mir/run/exc_caf_thread.mir` (TSan)。
  - Core Lint: ハンドラが投げ直す `catch` を発散とみなし、その枝の線形性を検査しなかった (`tests/core/fail/catch_reraise_branch.core`)。
    同じ穴の一般の形 (本体の発散するラムダ) は BUG-0047 に起票した。
  - 正格性解析: `catch` の後の要求を `catch` の前へ伝え、`strictify` と worker/wrapper が値を `catch` の前で評価した
    (例外が `catch` の外で起こり、ハンドラが動かない)。`catch` を走らせうる評価を境にした (ADR-128、06-ir.md §1.8。
    `tests/core/run/catch_strict.core`、単体テスト `strictify_does_not_move_evaluation_before_a_catch`)。この試験で、
    遅延の構築子が `!` のない unboxed フィールドを束縛の時点で計算する BUG-0046 が見つかり、直した。
- ミューテーション (修正をひとつずつ戻す 9 通り): すべてテストが失敗する。CAF の例外の共有を戻した版は MIR の実行テスト
  `exc_caf_thread` だけでは失敗しなかったので、ランタイムの単体テストで共有マークを直接確かめる。
- 計測の途中で、演算子の名前の関数を含む MIR のテキストが読み戻せないことが分かった (BUG-0045)。
