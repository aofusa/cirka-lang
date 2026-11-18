# FEAT-0262: セッション型

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.7 効果・能力・セッション型
- 優先度: P2
- 依存: FEAT-0240
- 設計: [13-verification.md](../../design/13-verification.md), [spec-v0.0.7.md](../../design/cirkalang/spec-v0.0.7.md) §3

## 概要

線形チャネルとプロトコル型。双対性の検査。

## 作業内容

- [x] Send/Recv/Offer/Choose/End と Dual
- [x] 例題 (クライアント/サーバ)

## 受け入れ条件

- [x] プロトコル違反・途中放棄が型エラー
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

## 結果

- `Std.Session` (`lib/cirkalang/std/Std/Session.cirka`、spec-v0.0.7.md §3、ADR-106): プロトコル `Session`
  (`Send`・`Recv`・`Choose`・`Offer`・ループの `Rec` / `Var`・`End`) と、型の中で評価される `dual` / `unroll`。
  端点 `Channel s` は線形で `Drop` を `Channel End` にだけ持ち、操作は `send`・`recv`・`chooseLeft` / `chooseRight`・`offer`・`enter`・`close`、
  始め方は `connect` (二つの側を実行して両方を待つ) と `fork` (相手をスレッドで始める)。
- 値は `Std.Thread` のチャネル 2 つで運び、段ごとの要素の型は、ライブラリだけが宣言できるプリミティブ `primUnsafeCastChan` で変える
  (共通 Core では何もしない)。
- プロトコルの違反・途中での放棄・二重の使用・双対でない相手は、既存の規則の型エラー (E1200・E0302・E0303) になる。
  診断を二つ改善した: E0302 は `Drop` / `Release` の実装が別の引数にだけある型 (終わっていない端点) で導出を案内しない。
  E1200 は型の中の関数を評価した形も示す (`dual p`。既存のテストでは `fail/proof/wrong.stderr` に、計算した形の注記が加わった)。
- テスト: `run/v007/session.cirka` (選択のあるループのクライアントとサーバ、書き下した双対)、`run/v007/session_fork.cirka`
  (`fork` した 2 つの相手と順に話す、いくつかの型の値、終わった端点を捨てて閉じる)、`fail/session/` (違反・放棄・双対でない相手)。
  スレッドを使うので MIR インタプリタでは実行せず、ThreadSanitizer でも実行する。
- `bench/session` (セッション型・同じ値を直接送る版・Rust)。`bench_programs` は `-- skip-backend: interp` のベンチをネイティブで実行する。
  計測は spec-v0.0.7.md §3.6 と bench-report.md。
- 作業中に見つけたもの: FEAT-0090 (`offer` の `Branch` と端点の確保。線形な値の case-of-case)、BUG-0037 (型が合わない引数の後の E1254)。

