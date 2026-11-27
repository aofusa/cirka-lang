# FEAT-1032: アンワインドの様式の費用を減らす

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0040
- 設計: [07-backend.md](../../design/07-backend.md) §2, [05-runtime.md](../../design/05-runtime.md) §6, [16-performance.md](../../design/16-performance.md)

## 概要

`catch` を含むプログラムはアンワインドの様式でコンパイルされ (ADR-125)、同じプログラムを中止の様式でコンパイルしたものより遅い。
FEAT-0040 の計測 (`--unwind` で `catch` のないベンチをアンワインドの様式にした) では、命令数が spectral-norm `plain.hs` で 4.6%、
fannkuch-redux `plain.hs` で 2.0%、cli-grep で 1.6% 増え、時間は 3〜6% 増えた (ほかの 13 個は −4.1〜+1.4%)。
aCHC の `Control.Exception` (FEAT-0145) と CirkaLang の panic のアンワインド (FEAT-0302) の後は、`catch` を使うプログラムがこの費用を払う。

増えた命令の場所 (cachegrind):

- **サンクの入口**: サンクのスタブは、例外を止めて保留の枠に置くために本体を `invoke` で呼ぶ (ランタイムの `force` の Rust のフレームは
  アンワインドしない、ADR-123)。中止の様式では本体への末尾呼び出しになるが、アンワインドの様式ではスタブがフレームを作る。
  spectral-norm `plain.hs` の増分はすべてこれ (`cirkaFoldrList` の 4 つの `$thk`)。
- **`invoke` と landing pad**: 投げうる呼び出しをまたいで参照を持つフレーム。fannkuch-redux `plain.hs` (`++`・`take`・`drop`・`zip` など) と
  cli-grep (`main`・`appendInt` の、投げうる extern の呼び出しをまたぐループ)。

## 作業内容

- [ ] 生成した機械語で、増える命令 (スタブのフレーム、landing pad のために生かす値の退避) を確かめる
- [ ] 候補を試す。例:
  - サンクの評価で例外を Rust のフレームを越えずに伝える (生成コードの `force` の遅い経路がスタブを直接呼ぶ、など。黒穴化と MT の同期を保つ)
  - `catch` の中で動きうるコードだけをアンワインドの様式にする (`catch` の `act` から呼び出しグラフで到達する関数。サンクと未知の関数は保守的に)
  - 後始末のない呼び出し・値を退避しなくてよい呼び出しを `call` のままにする範囲を広げる
- [ ] 設計書 (07-backend.md §2、05-runtime.md §6、ADR) の更新

## 受け入れ条件

- [ ] アンワインドの様式の命令数の増分が減る (spectral-norm `plain.hs`・fannkuch-redux `plain.hs`・cli-grep で計測)
- [ ] 例外の意味 (後始末・リーク 0・捕まらない例外の扱い) が変わらない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- 計測の手順: `docs/artifacts/feat0040/unwind-retime.sh` (git 管理外) と同じく、`--unwind` の有無で作った 2 つの実行ファイルを
  cachegrind (`--cache-sim=no`) で数え、`cg_annotate --diff` で関数ごとの増分を見る。
