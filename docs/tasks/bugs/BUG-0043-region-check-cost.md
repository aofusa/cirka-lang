# BUG-0043: BUG-0042 の修正で解放ごとの領域の判定が重くなり、binary-trees が 4% 遅くなった

- 状態: done
- 重大度: minor
- 発見箇所: FEAT-1006 の全ベンチの計測 (`docs/artifacts/f1006/ab`)
- 関連: BUG-0042, FEAT-0091, FEAT-0096

## 現象

FEAT-1001〜1007・BUG-0042 をまとめた全ベンチの計測で、binary-trees だけが main より遅い (負荷の低い 3 回で
1.037〜1.055 倍)。生成したコードの木を作る・調べる関数 (`Main.make$w`・`Main.check$w`・`Main.iterate$w`) の LLVM IR は
main と同じで、違いはランタイムの解放 (`object::release` に入った `alloc::dealloc`) にある。

## 再現手順

```sh
docs/artifacts/perf/abrun.sh docs/artifacts/f1006/ab docs/artifacts/f1000/ab -new binary-trees   # コンテナ内
```

## 期待される動作

BUG-0042 の前と同じか速い解放。

## 原因

BUG-0042 で領域の大きさが定数 `REGION` から状態のフィールド `size` になり、`dealloc` の判定
`mode == 1 && a >= base && a < base + size` が、モードとベースの比較を `setcc` で合わせたうえでフィールドを読み、
足して比べるようになった (解放 1 回に 3 命令ほど増える)。binary-trees は 6 億回ほど解放する。

## 修正

- [x] 回帰テストを追加 (`region_membership_is_one_comparison`: 判定の境界・ベースより下 (折り返し)・使わない領域)
- [x] 修正: 判定を `a.wrapping_sub(base) < size` の 1 回の符号なし比較にする (`in_region`)。領域を使わないモードでは
  `size` が 0 なのでモードを読まない。範囲内のページは表の中にあり、クラスは `CLASSES` 未満なので、表と
  フリーリストの添字の検査を省く (`get_unchecked`)
- [x] binary-trees を計測して記録する

## メモ
- 2026-11-25: binary-trees (5 回の交互実行の中央値)。修正の後は FEAT-1006 のビルド (BUG-0042 の判定) の 0.933 倍 (負荷 2.3、
  最小値の比 0.984)、BUG-0042 の前のビルド (f1000) の 0.957 倍 (負荷 3.5、最小値の比 0.953)。生成コードは同じで、違いは解放の判定だけ。
