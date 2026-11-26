# FEAT-0040: 例外/パニックのアンワインド

- 状態: in-progress
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0031
- 設計: [05-runtime.md](../../design/05-runtime.md) §6, [06-ir.md](../../design/06-ir.md) §1.8・§2.5, [07-backend.md](../../design/07-backend.md) §2, ADR-123〜127

## 概要

Itanium ABI のアンワインドと landing pad での drop。共通 Core・MIR・LLVM・ランタイムに例外の投げ方 (`raise`) と捕まえ方 (`catch`) を入れ、
例外の経路でも所有の参照を解放する (リーク 0)。`catch` を含まないプログラムは今と同じコード (中止の様式、ADR-125) で、性能は変わらない。
aCHC の `Control.Exception` (`catch`/`bracket`/`finally`/`throwIO`) と W0305 は FEAT-0145、CirkaLang の panic のアンワインドと
`Drop`/`Release` の実行は v0.1.0 の FEAT-0302 で、この共通基盤の上に作る。

## 作業内容

- [ ] MIR: 終端命令 `raise x`、`let (st, v) = catch apply c(args)`、テキスト形式、検証器、様式 (`Module::catches`)
- [ ] MIR: 投げうる関数 (`unwind::may_raise`) と後始末の集合 (`unwind::cleanups`、最終の MIR の所有の数)
- [ ] MIR インタプリタ: `Exit::Raise`、後始末、`catch apply`、例外を覚えるサンク、捕まらない例外の表示と `RunResult::orderly`
- [ ] ランタイム: パーソナリティ関数、例外の枠と保留の枠、`cirka_exception_*`・`cirka_uncaught*`、様式の印、例外を覚えるサンク、
      `<<loop>>`・`cirka_panic_bytes`・ファイルの誤り・バイト列の範囲外を保留の枠で返す
- [ ] コード生成: `nounwind` の除去、`invoke` と landing pad、冷たいブロックでの後始末と投げ直し、`catch apply`、入口 (`cirka_main`・
      サンクのスタブ・`cirka_call_1`・C のコールバック)、`apply` の汎用の関数、24 バイトのサンク
- [ ] Core: プリミティブ `raise`・`catch`、Core Lint、簡約器・正格性解析での扱い、MIR への変換
- [ ] テスト: MIR の実行テスト (捕まえる・捕まらない・後始末・サンク・CAF、インタプリタとネイティブの一致)、Core の実行テスト
      (`catch` で作った解放処理が例外の経路で動くことを出力で確認)、コード生成の単体テスト、ランタイムの単体テスト
- [ ] 性能: 全ベンチで中止の様式の生成コードが変わらないことを確かめ、アンワインドの様式の費用を計測する

## 受け入れ条件

- [ ] 例外発生時に解放処理が実行されることを出力で確認 (Core の `catch` で作った解放処理)
- [ ] 例外経路でもリーク 0 (捕まえた例外・捕まらなかった例外の両方、インタプリタとネイティブの確保・解放の回数が一致)
- [ ] `catch` を含まないプログラムの生成コードと性能が変わらない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- 試作 (docs/artifacts/f0040/exp1): Rust (no_std) のパーソナリティ関数と LLVM の `invoke`/`landingpad`/`resume` で、`tailcc` と `musttail` の
  フレームを越えてアンワインドできる。clang のリンクは libgcc_s を自動で付ける。panic=abort の Rust の `extern "C-unwind"` の関数は
  アンワインドを中止する landing pad (`rust_eh_personality`) を持つので、Rust のフレームはアンワインドしない (ADR-123)。
- W0305 (例外の経路で漏れうる線形の値) は aCHC の `.chs` の規則なので FEAT-0145 に移した。
