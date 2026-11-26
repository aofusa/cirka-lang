# FEAT-0302: panic のアンワインドと Drop / Release の実行 (CirkaLang v0.1.0)

- 状態: todo
- トラック: 第二段階
- マイルストーン: P2 第二段階 (CirkaLang v0.1.0 以降)
- 優先度: P1
- 依存: FEAT-0040, FEAT-0300
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md), [05-runtime.md](../../design/05-runtime.md) §6

## 概要

CirkaLang の `panic` を FEAT-0040 のアンワインドで投げ、スコープの値の `Drop`/`Release` を例外の経路でも実行する (12-cirkalang.md の
約束)。v0.0.1〜v0.0.7 の仕様は panic を abort と定めているので、v0.1.0 の仕様 (FEAT-0300) で意味を改め、CHANGELOG に記録する。

## 作業内容

- [ ] v0.1.0 の仕様: panic の意味、捕まえる場所 (テストの実行・スレッドの境界)、スレッドの panic と join・セッション型の活性・同期の補助
      (`modifyMVar`・`withMutex`) の扱い (ADR-127 の見直し)
- [ ] `panic` とコンパイラが作る panic を `raise` にする
- [ ] `dropAfter`・`releaseAfter` を例外に対して安全にし、elab の RAII が例外の辺でも値を終える
- [ ] 総関数の検査と `catch` の型 (線形の値を捕捉できない)

## 受け入れ条件

- [ ] panic で `Drop`/`Release` が実行されることを出力で確認、例外の経路でもリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
