# 14. aCHC 後期拡張: 高度な型・依存型・検証・効果

aCHC (試作A) は、初期の検証 (GC なしで Haskell が動くか・所有権拡張の書き味: H1–H4) の後、
**CirkaLang で検証済みの** 依存型・検証機能・効果システムを Haskell の拡張として取り込む (ADR-027)。

## 1. 順序と前提

```
aCHC H1–H4 (GC なし・所有権)  ──────────────────────────────┐
CirkaLang v0.0.4 (依存型・証明・検証) ───┐                    │
                                       ├─► aCHC H5 高度な型 ─► aCHC H6 依存型・検証 ─┐
CirkaLang v0.0.7 (効果・能力・セッション) ─────────────────────────────────────────► aCHC H7 効果・能力・セッション
```

| 原則 | 内容 |
|------|------|
| 最優先は GC なしと所有権 | H5 以降は H1–H4 (評価レポート FEAT-0132 を含む) の完了後に着手する |
| CirkaLang で先に検証する | 依存型・検証は CirkaLang v0.0.4、効果・能力・セッション型は v0.0.7 のリリース後に、その評価を踏まえて aCHC に入れる |
| Haskell の意味を変えない | 追加する機能はすべて GHC の拡張・提案、Liquid Haskell、既存の効果ライブラリの流儀に沿い、`.hs` の意味を変えない (TypeScript の考え方を維持) |
| 実行時コストゼロ | 型・証明・述語は消去される (多重度 0)。効果は evidence passing で末尾再開形のハンドラを関数呼び出しと同じコストにする |
| 共通基盤を再利用する | `cirka-smt`、証明済み事実による検査除去、効果ハンドラの Core 上の変換は CirkaLang と共有する |

## 2. H5: 高度な型システム

GHC と同じ構文・意味で次の拡張を扱う (02 §1.2 の後期分)。

| 拡張 | 内容 | 実装上の要点 |
|------|------|--------------|
| `GADTs`, `ExistentialQuantification` | 構築子ごとの型の精密化、存在型 | 局所的な型等式制約 (OutsideIn(X) の簡易版)。GADT のマッチには型注釈を要求する |
| `TypeFamilies` | open / closed / 関連型族、データ族 | 制約ソルバでの型族の簡約。単射性は後回し |
| `DataKinds`, `PolyKinds`, `KindSignatures` | 構築子の昇格、型レベルのリスト・タプル、種多相 | 種 `Type`/`LType` と昇格した種の共存 (LType は昇格しない) |
| `MultiParamTypeClasses`, `FunctionalDependencies` | 関数従属による改良 | 制約ソルバの improvement |
| `TypeOperators`, `ConstraintKinds`, `RankNTypes` (完全) | | |
| 型レベル `Nat` / `Symbol` (`GHC.TypeLits` 互換) | 型レベルの算術・文字列 | 組み込みの算術正規化ソルバ (Presburger の範囲)、`KnownNat` |

**Core での表現**: GADT や型族による型等式は、共通 Core では CirkaLang の依存型と同じく `Any` / `cast` で表す (ADR-021)。
型等式の正しさはフロントエンドの型検査器が保証し、Core Lint は `cast` の制約 (boxed 型間のみ) だけを検査する。
将来、Core Lint の検査力を上げる必要があれば、等式の証拠 (coercion) を Core に追加することを検討する。

### 2.1 GADTs と存在型 (FEAT-0140)

**構文**: GADT 構文 (`data T a where C :: … -> T τ`。複数名の `C1, C2 :: …`、`data T :: Type -> Type where`、空の宣言も) と、
存在型の構文 (`data T = forall a. Show a => MkT a`。正格の印・中置構築子・レコードも) を受け付ける。どちらも以前は構文エラーだったので、
受け付けていたプログラムの意味は変わらない。`GADTs`・`GADTSyntax`・`ExistentialQuantification`・`ExplicitForAll` の pragma は
受け付けるが、構文を切り替えない (`forall`・`\case` と同じ)。

**正規形**: 型検査器は、どちらの構文で書いた構築子も GHC と同じ形に直す:
`K :: forall params exs. ctx => fields -> T params` と、局所的な型等式 `eqs` (パラメータの位置 i と型 τ の組)。
結果の型 `T ρ1 … ρn` の ρi がまだ使っていない型変数ならそれを i 番目のパラメータとし、そうでなければ (i, ρi) を等式にする。
残りの型変数が存在型の変数。`exs`・`ctx`・`eqs` がすべて空の構築子 (vanilla) は従来と同じ経路で扱うので、
`data T a where C :: a -> T a` は `data T a = C a` と同じ Core になる。

**構築**: パラメータと存在型の変数をメタ変数で具体化し、等式は単一化で確かめ、文脈の制約は辞書の要求にして構築子の先頭の
隠れた引数にする。要求は外側の汎化点で解くので、`pack x = MkT x` は `Show a => a -> T` と推論される。

**照合 (分岐のスコープ)**: パターンが vanilla でない構築子を含む節 (`case` の選択肢・関数の節・ラムダ・`\case`・`do` の束縛・
リスト内包の生成器) だけに分岐のスコープを開く。含まない節の型検査は変わらない。

- 存在型の変数は新しい剛体変数にする。分岐の外へ出ると E0621。
- 詰めた辞書は、分岐の中で与えられた制約になる。
- GADT の等式は、被検査式の型の引数 (剛体変数) から構築子の型への **局所的な置換 σ** として、分岐の中だけで有効にする
  (OutsideIn(X) の簡易版)。単一化は、剛体変数で失敗するところでだけ σ を見る。
- σ があるあいだ、外側のメタ変数には触れない (untouchable)。それを束縛する必要があれば E0620 で型シグネチャを促す。
  含意制約の遅延はしない。GADT の照合には、等式の位置で被検査式の型が分かっている必要がある。
- 等式が矛盾する選択肢 (`Expr Int` の値に対する `BoolE`) には到達しない。W0623 を出し、右辺を実行時エラーにする (被検査式は評価する)。

遅延パターン・`let`/`where` のパターン束縛 (GHC と同じ) とパターンガード (この版の制限) の中では、これらの構築子は E0622。

**証拠と Core**: 分岐の右辺の型は σ を適用した形で型付き AST に書き、σ を適用した型とそうでない型の境目に `Coerce`
(Core の `cast`。MIR で消えるので実行時のコストはない) を置く。境目は次の 4 種類:

1. 分岐の値と、選択肢全体の型
2. 分岐の外で束縛した局所変数の参照
3. σ を使って解いた辞書 (与えられた制約を σ のもとで使った所)
4. σ で等しい型での、後続の列の照合 (`Pat::Coerce`)

Core では、存在型の変数を `Any`、文脈の辞書を `C Any` 型のフィールドにする。精密化されるパラメータは通常のデータのパラメータのまま
(06-ir.md §1.1)。Core Lint は `cast` の制約 (箱型どうし) だけを検査するので、等式の健全性は型検査器が保証する。
境目の `cast` の書き漏らしは、デバッグビルドの Core Lint が ICE として見つける。

```
eval :: Expr a -> a            -- IntE :: Int -> Expr Int など
eval (IntE n) = n              --   ((Main.IntE n) (cast n a))
data Showable = forall a. Show a => MkShowable a
                               --   (data Main.Showable () (Main.MkShowable (Cirka.Base.Show Any) Any))
```

**網羅性**: 列の型の引数と等式が矛盾する構築子は、網羅性の検査と `case` の既定の選択肢から除く (`Expr Int` の照合に `BoolE` は要らない)。
列をまたぐ精密化は使わないので、まれに不要な W0610 が残る。

**この版の制限 (E0624)**: vanilla でない newtype、vanilla でない構築子のレコード構文、高階の種や LType の存在型の変数、
`->`・`⊸`・`%1 ->` 以外の多重度のフィールド、構築子の文脈の外の `~`。vanilla でない構築子を持つ型の `deriving` は E0615。
型族・DataKinds などとの組み合わせは FEAT-0141 以降。

## 3. H6: 依存型と検証

### 3.1 依存型 (Dependent Haskell の方向に沿う最小限)

GHC の Dependent Haskell 関連の提案に沿い、CirkaLang の QTT で得た知見 (0 = 消去、1 = 線形、ω = 共有) を当てはめる。

```haskell
{-# LANGUAGE CirkaDependent #-}

-- 必須の型引数 (GHC の RequiredTypeArguments と同じ)
sizeOf :: forall a -> Storable a => Int

-- 依存量化: n は型にも値にも現れる。実行時には n の値 (シングルトン) が渡される
replicate :: foreach (n :: Nat) -> a -> Vect n a

-- 消去される量化 (従来の forall と同じ。多重度 0)
vlength :: forall n a. Vect n a -> Proxy n
```

- `foreach` (関連する依存量化) は、型検査後に **シングルトンの受け渡しへ elaborate** する (singletons ライブラリの手法を自動化)。
  `Cirka.Singletons` に `SNat`、`SBool`、`SList` などと、データ型からシングルトンを導出する `deriving Sing` を用意する。
- 型レベルで使う関数は型族として書く (H5)。値レベルの関数を型族へ自動昇格する仕組み (singletons の `promote`) は、CirkaLang の評価を見て判断する。
- 多重度は既存の `%0` / `%1` / `ω` をそのまま使い、`foreach` の引数に多重度を付けられる (`foreach (n :: Nat) %1 ->`)。

### 3.2 検証

13-verification.md で CirkaLang 向けに設計したものを、Haskell の流儀で提供する。

| 機能 | aCHC での形 |
|------|-------------|
| 洗練型 | Liquid Haskell 互換の `{-@ ... @-}` 注釈 (`.hs` でも書ける。TypeScript の `--checkJs` に相当) と、`.chs` のネイティブ構文 `{v : τ \| p}` / `(x : τ) ->` (`CirkaRefinement`) |
| 契約 | `.chs` の `requires` / `ensures` / `invariant` (`CirkaContracts`)、`--contracts=off\|check\|verify` |
| 全域性・終了性 | `{-@ total f @-}`、尺度 `/ [n]`。証明に使う関数は全域必須 |
| 証明 | Liquid Haskell の reflection 方式 (`reflect`、等式推論の関数) と、H6 の依存型による証明 |
| 検査除去 | 証明済みの事実で境界検査などを除去 (共通基盤、ADR-019) |

- 検証は型検査が通った Core 上の独立パス (Liquid Haskell と同じ) で、`cirka-smt` を使う。
- **遅延評価への配慮**: 洗練型の健全性には束縛の評価が停止することが必要。正格な束縛・`LType` の値・全域性検査を通った関数の結果だけを仮定に使う (Liquid Haskell と同じ扱い)。

## 4. H7: 効果・能力・セッション型

### 4.1 ケイパビリティ

`Cirka.Capability` (能力トークン) と、`{-# CIRKA capability-safe #-}` モジュールでの生の IO の禁止。CirkaLang v0.0.7 の設計に合わせる。

### 4.2 効果システム

Haskell 互換を保つため、効果は **モナド `Eff es a`** として提供する (effectful / polysemy の流儀)。関数の型の形は変わらないので `.hs` とも共存できる。

```haskell
{-# LANGUAGE CirkaEffects #-}

data State s :: Effect where
  Get :: State s m s
  Put :: s -> State s m ()

counter :: (State Int :> es, Console :> es) => Eff es ()
counter = do
  n <- send Get
  send (Put (n + 1))
  send (Print (show n))

main :: IO ()
main = runEff . runConsole . evalState 0 $ counter
```

- 効果の列 `es :: [Effect]` は DataKinds の型レベルリスト (H5)。所属制約 `e :> es` は **組み込みのソルバ** で解き、重複インスタンスに頼らない。
- ハンドラは **evidence passing** でコンパイルする。CirkaLang v0.0.7 (FEAT-0261) で実装する共通 Core 上の変換を再利用し、
  末尾再開形のハンドラ (State・Reader・Writer の多く) は通常の関数呼び出しと同じコストにする。
- 線形な資源と効果の相互作用 (ハンドラが継続を複数回呼ぶ場合の線形値) は CirkaLang の評価結果に従って制限する (継続の多重呼び出しを ω の効果に限定する等)。

### 4.3 セッション型

`Cirka.Session` (線形チャネル + GADTs + DataKinds)。aCHC の並行性 (`forkIO` / `MVar`、共通基盤 FEAT-0045 の上: FEAT-0163) を前提にする。

## 5. 引き続き対象外

- Hackage パッケージの移植と GHC testsuite の合格率目標 (計測は行う)
- Template Haskell、GHCi、非同期例外
- GHC の全拡張の網羅 (CirkaLang の検証に対応するもの、および差分テストに必要なものに限る)
