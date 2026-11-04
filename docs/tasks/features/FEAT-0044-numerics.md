# FEAT-0044: 数値型と Integer

- 状態: todo
- マイルストーン: M4 Haskell 実用化
- 優先度: P0
- 依存: FEAT-0041
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

Int/Word/IntN/WordN/Double/Float/Integer/Rational と数値型クラス、show/read の GHC 互換、libm 呼び出し。

## 作業内容

- [ ] ランタイムの多倍長整数 (加減乗除・divMod・gcd・べき・show)
- [ ] Num/Integral/Fractional/Floating/RealFrac/RealFloat
- [ ] 浮動小数の show/read を GHC と一致
- [ ] libm の FFI 呼び出し

## 受け入れ条件

- [ ] `product [1..100] :: Integer` 等の差分テスト
- [ ] 浮動小数の表示が GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

