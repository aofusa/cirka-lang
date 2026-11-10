# FEAT-0077: 正格性解析が newtype のコンストラクタを素通しする

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0112
- 設計: [06-ir.md](../../design/06-ir.md) §1.5

## 概要

`Unique (UArray Double)` のように newtype で包まれた配列を返す式は、コンストラクタの適用が値に見えるため `strictify` の対象にならず、
関数に渡すときにサンクになっていた。n-body の `steps n (U.thaw a1)` は `U.thaw a1` が `let` のサンクのまま `steps` に渡る。

newtype のコンストラクタは表現を持たない (フィールドそのもの) ので、それを WHNF まで評価すればフィールドも評価される。正格性解析の `Con` の扱いに newtype を加える。

## 作業内容

- [x] `ww.rs` の解析: newtype のフィールドは正格
- [x] 単体テスト (newtype は評価が早まり、普通のコンストラクタは遅延のまま)
- [x] ベンチで前後を計測する

## 受け入れ条件

- [x] 上記のテストがある
- [x] 全ベンチの出力が変わらず、命令数が増えない (計測をメモに記録)
- [x] `scripts/ci.sh` が通る

## メモ

命令数 (callgrind Ir、FEAT-0075 → この変更だけ、own.chs):

| ベンチ | FEAT-0075 | FEAT-0077 |
|---|---:|---:|
| n-body 500k | 232.4 M | 230.4 M |
| fannkuch-redux 9 | 152.0 M | 151.6 M |
| array-sort 1M | 303.9 M | 303.9 M |
| spectral-norm 300 | 68.3 M | 68.3 M |

n-body は `U.thaw a1` の結果が `steps` にサンクではなく値で渡るようになった。
これは FEAT-0076 の前提でもある。サンクのままだとループの入口の検査に失敗し、反復が冷たい経路で回って 296.9 M まで悪化していた (FEAT-0076 のメモ)。

ミューテーション: 解析の `|| newtype` を除くと `strictify_sees_through_newtype_constructors` が失敗する。

`strictify` の `Mult::Many` の条件 (線形の束縛は早めない) は外しても命令数が変わらなかったので残した。必要になったら別のチケットで扱う。
