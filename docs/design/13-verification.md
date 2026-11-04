# 13. 検証機能: 仕様と正しさの静的保証

入力: [docs/specs/03-dependent-types.md](../specs/03-dependent-types.md)、[docs/specs/04-features.md](../specs/04-features.md)

00/01 の「GCなし・Rust 並みの性能」とは **別の軸** の機能群である。
プログラムが仕様を満たしていること、バグがないことをコンパイル時に確認する。
**優先度は 00/01 の軸より低い**(マイルストーン M9。M5 完了以降に並行して進める)。

## 1. 基本原則

| 原則 | 内容 |
|------|------|
| **P1. 実行時コストゼロ** | 仕様・証明・洗練述語はすべて多重度 `%0` 相当で **消去** される。生成コードに残るのは、利用者が明示的に選んだ実行時検査 (`--contracts=check`) だけ |
| **P2. 任意 (opt-in)** | 注釈を書かないコードには何の影響もない。検証パスは `achc verify` または `--verify` を指定したときだけ走り、通常のビルド時間を増やさない |
| **P3. 既存の意味を変えない** | 検証注釈は Haskell の意味論を変えない(TypeScript の型注釈と同じ)。注釈を消しても同じプログラム |
| **P4. 自動化優先** | 手で証明を書くより、SMT ソルバ (Z3) による自動検証を優先する。証明の手書きは最後の手段 |
| **P5. 性能にはむしろ効く** | 証明できた事実(添字が範囲内、除数が非ゼロ、パターンが網羅)を使って、実行時の境界検査などを **除去** できる |
| **P6. 信頼の範囲を明示** | `assume`、FFI 宣言の述語、SMT ソルバ自体が信頼基盤。`achc verify --report` で仮定の一覧を出す(Lean の `sorry` 数に相当) |

## 2. 取り込む機能と段階

specs 03/04 で挙げた機能ごとに、第一段階 (aCHC: Haskell 拡張) と第二段階 (CirkaLang: 言語機能) での扱いを決める。

| 機能 | 第一段階 (aCHC) | 第二段階 (CirkaLang) | 優先度 |
|------|-----------------|----------------------|--------|
| **洗練型 (Refinement / Liquid Types)** | ◎ Liquid Haskell 互換の `{-@ @-}` 注釈 + `.chs` のネイティブ構文。Core 上の独立した検証パス + Z3 | 型検査器に統合 | 最優先 |
| **契約 (事前/事後条件・不変条件)** | ◎ 洗練型への糖衣 (`requires` / `ensures` / `invariant`)。`--contracts=check` で実行時検査も可能 | 同左 (言語構文) | 最優先 |
| **終了性・全域性の検査** | ○ 構造的再帰 + 減少する尺度 (`/ [n]`)。証明の健全性に必要 | 同左 | 高 |
| **量的型理論 (QTT: 0/1/ω)** | ○ 多重度 `%0`/`%1`/`ω` は既に設計済み (02 §2)。証明項を `%0` で消去 | 依存型の Core の基礎 | 高 (設計は済) |
| **依存型** | △ **軽量版**: DataKinds・GADTs・型族・型レベル自然数ソルバ・シングルトン (`Vect n a`, `Fin n`, `Array n e`) | ◎ QTT に基づく完全な依存型 (Idris 2 方式) | 中 |
| **ケイパビリティ (効果の能力トークン)** | ○ ライブラリ (`Cirka.Capability`) + 「生の IO 禁止」モジュール検査 | 効果システムに統合 | 中 |
| **効果システム / 代数的効果** | × (Haskell の `IO`・モナドのまま) | ○ Koka 風の行多相な効果型。ハンドラは末尾再開形を優先 (性能維持) | 中 (第二段階) |
| **セッション型** | △ ライブラリ (`Cirka.Session`: 線形チャネル + GADTs)。並行性 (M8) 後 | ○ 双対性検査を組み込み | 低 |
| **対話的証明 (Agda/Lean 級)** | × | × (コアには入れない。将来 Lean への仕様書き出しを検討) | 様子見 |

◎ 本格対応 / ○ 対応 / △ 限定対応 / × 対応しない

**理由の要点**:

- 洗練型は「Haskell の型推論を変えずに後から載せられる」ことが Liquid Haskell で実証されており、第一段階の「Haskell 拡張」という制約に最も合う。
- 完全な依存型は Haskell の型推論・型クラスと根本的に衝突し (GHC の DependentHaskell も未完成)、第一段階で作ると Haskell フロントエンドを作り直すことになる。第一段階は GHC と同程度の軽量版にとどめ、本命は第二段階で Core から作る。
- 効果システムは関数の型の形そのものを変えるため、Haskell 互換と両立しない。第二段階で導入する。

## 3. 洗練型 (第一段階の中心)

### 3.1 構文

**(a) Liquid Haskell 互換の注釈コメント**: `.hs` でも使える。Haskell としては単なるコメントなので、GHC でもそのままコンパイルできる。
TypeScript で JSDoc の型注釈を `.js` に書いて `tsc --checkJs` で検査するのと同じ位置づけ。

```haskell
{-@ type Nat = {v:Int | 0 <= v} @-}
{-@ measure len @-}
{-@ safeDiv :: Int -> {d:Int | d /= 0} -> Int @-}
safeDiv :: Int -> Int -> Int
safeDiv n d = n `div` d

{-@ index :: xs:[a] -> {i:Nat | i < len xs} -> a @-}
```

Liquid Haskell の構文のうち、`type`・`measure`・`reflect`・`inline`・`predicate`・関数シグネチャ・`data` の洗練・終了尺度 `/ [..]` を対応範囲とする。

**(b) `.chs` のネイティブ構文** (`{-# LANGUAGE CirkaRefinement #-}`):

```haskell
type Nat = {v : Int | v >= 0}

safeDiv :: Int -> {d : Int | d /= 0} -> Int

-- 引数に名前を付けて後続の型から参照する (依存関数型の記法)
take :: (n : Nat) -> (xs : [a]) -> {ys : [a] | len ys == min n (len xs)}
```

`(x : τ) ->` の単一コロンは、この拡張が有効なときだけ束縛子として解釈する(`DataKinds` の型レベル cons とは文脈で区別。曖昧な場合はエラー)。

### 3.2 述語の論理

決定可能な断片に限定し、SMT で自動的に解ける範囲に保つ:

- 線形整数算術、真偽値、等号
- 未解釈関数、**measure**(ADT 上の構造的再帰関数を論理へ持ち上げたもの: `len`, `size`, `elems`)
- `reflect` した小さな全域関数(定義を論理に展開)
- 配列の理論(`Array` の要素について述べる場合)

### 3.3 検証の仕組み

```
Typed AST ─► Core (最適化前) ─► cirka-verify: 検証条件 (VC) 生成 ─► SMT-LIB2 ─► Z3 (外部プロセス)
                                     │                                     │
                                     └── 洗練型の推論 (Liquid: 述語抽象)   └── 結果キャッシュ (_build/verify/)
```

- 検証は **通常の型検査が通った後の Core** 上で、独立したパスとして行う(Liquid Haskell が GHC Core 上で行うのと同じ)。型推論・型クラスの解決には一切影響しない。
- 部分型付け `{v:τ | p} <: {v:τ | q}` は `p ⇒ q` の VC になり、Z3 で妥当性を確認する。
- 注釈のない中間の関数・let の洗練は **Liquid 型推論**(修飾子からの述語抽象による不動点計算)で推論し、注釈の量を減らす (FEAT-0093)。
- 失敗時は反例 (Z3 のモデル) を、ソース上の変数名で表示する。
- 外部ソルバのパスは `CIRKA_Z3`、タイムアウトは `CIRKA_VERIFY_TIMEOUT`。

### 3.4 所有権・線形性との統合

所有権と洗練型は互いを強める (Rust 向けの Flux / Verus と同じ発想):

```haskell
-- 一意配列: 長さは書き込みで変わらないことを型で表す
write :: (a : Unique (Array e)) %1 -> {i : Int | 0 <= i && i < len a} -> e
      -> {b : Unique (Array e) | len b == len a}

-- 借用: 借用中は不変なので measure をそのまま使える
index :: (a : Ref r (Array e)) -> {i : Int | 0 <= i && i < len a} -> Borrow r e
```

- LType の値は一意なので、「消費前の値」と「返ってきた新しい値」の関係を述語で書ける(エイリアスがないため、可変状態の推論が単純になる)。
- 証明できた範囲内アクセスは、コード生成時に **境界検査なしの命令** になる (FEAT-0096)。

### 3.5 遅延評価との関係

洗練型の健全性には「束縛された値の評価が停止する」ことが必要(発散する値は任意の述語を満たしてしまう)。

- 正格な束縛・LType の値・終了性検査 (§5) を通った関数の結果は、通常どおり仮定として使う。
- 終了性が分からない遅延束縛は、その述語を他の VC の仮定に使わない(Liquid Haskell と同じ扱い)。

## 4. 契約 (Contracts)

洗練型の糖衣として、Dafny / SPARK / Verus 風の書き方を提供する。

```haskell
binarySearch :: (arr : Ref r (Array Int)) -> (key : Int) -> Borrow r (Maybe Int)
  requires sorted arr
  ensures  \res -> case res of
             Just i  -> 0 <= i && i < len arr && arr ! i == key
             Nothing -> not (key `elem` arr)

data Range = Range { lo :: Int, hi :: Int }
  invariant lo <= hi
```

- `requires p` は引数の洗練、`ensures q` は結果の洗練、`invariant` は構築子の型の洗練に脱糖する。
- `--contracts=off|check|verify`:

| モード | 動作 | 実行時コスト |
|--------|------|--------------|
| `off` (release 既定) | 何もしない | なし |
| `check` (debug ビルド向け) | 静的に証明できなかった契約を **実行時アサーション** として挿入 | あり (debug のみ) |
| `verify` | SMT で静的に検証し、失敗はコンパイルエラー (`ownership` と同様に warn/error を選択可) | なし |

`check` と `verify` は併用でき、「証明できたものは消去、できなかったものだけ実行時検査」にできる。

## 5. 終了性・全域性

- 構造的再帰は自動で認める。それ以外は減少する尺度を注釈する: `{-@ go :: n:Nat -> Int / [n] @-}`。
- パターンの網羅性 (既に FEAT-0035 で警告) と終了性を合わせて **全域 (total)** と判定する。`{-@ total f @-}` または `.chs` の `total f` で全域性を要求できる。
- 証明として使う関数(`reflect` するもの、`%0` の証明項を作るもの)は全域でなければならない。

## 6. 軽量な依存型 (第一段階)

GHC と同程度の「型レベルプログラミング」を提供する。新しい型理論は入れない。

- `DataKinds`, `GADTs`, `TypeFamilies`, `KindSignatures`, `RankNTypes`(FEAT-0075 で実装)。
- **型レベル自然数ソルバ**: `n + 0 ~ n`、`(n + m) + k ~ n + (m + k)` などの算術等式を、型検査器に組み込んだ正規化 (Presburger の範囲) で解く。`--verify` 時は SMT にも委譲できる。
- `Cirka.Nat`: `SNat n`(シングルトン)、`Fin n`、`Vect n a`、サイズ付き一意配列 `SArray n e`。

```haskell
data Vect (n :: Nat) a where
  VNil  :: Vect 0 a
  VCons :: a -> Vect n a -> Vect (n + 1) a

append :: Vect n a -> Vect m a -> Vect (n + m) a
get    :: Fin n -> Vect n a -> a                  -- 範囲外アクセスが型で不可能
```

- 型引数は既に消去されるので実行時コストはない。値レベルの長さが必要な場合だけ `SNat n` を渡す。
- 洗練型と依存型は使い分ける: 長さなどの数値の性質は洗練型が手軽、データ構造の形(平衡木など)を型で強制したい場合は GADTs。

## 7. ケイパビリティ (第一段階はライブラリ)

```haskell
-- Cirka.Capability
type Cap :: Effect -> Type
data Effect = FileSystem | Network | Clock | Random | Process

withCapabilities :: (Caps -> IO a) -> IO a           -- main でのみ取得できる
readFileC :: Cap FileSystem -> FilePath -> IO Text   -- 能力を引数で要求
```

- 関数の型を見れば、どの能力(副作用の種類)を使うかが分かる。テストでは偽の能力を渡せる。
- `{-# CIRKA capability-safe #-}` を付けたモジュールでは、能力を取らない生の IO プリミティブ(`readFile` 等)の使用を禁止する(Safe Haskell と同様のモジュール単位の検査)。
- 能力を LType にすれば「一度だけ使える能力」(例: 一回限りの初期化)も表現できる。
- 第二段階の効果システムへの移行時に、能力 = 効果の証拠 (evidence) として自然に対応づく。

## 8. セッション型 (第一段階はライブラリ)

線形型と GADTs で表現する。並行性 (FEAT-0080) の後に実装する。

```haskell
type Proto = Send Int (Recv Bool End)
send :: Chan (Send a p) %1 -> a -> IO (Chan p)
recv :: Chan (Recv a p) %1 -> IO (a, Chan p)
close :: Chan End %1 -> IO ()
fork :: (Chan (Dual p) %1 -> IO ()) %1 -> IO (Chan p)
```

チャネルは LType なので、プロトコルの途中で捨てる・二重に使うことは型エラー(Release なし = 必ず `close` まで進める)。

## 9. 第二段階での統合

- **Core を QTT ベースの依存型付き Core に拡張**: 第一段階の多重度 `0 / 1 / ω` は QTT の半環そのものなので、Π 型の束縛子に多重度を付ける形で自然に拡張できる。`0` の項(型・証明)は消去される。
- 洗練型・契約は型検査器に統合し(双方向型検査 + SMT)、第一段階の `cirka-verify` の VC 生成と Z3 連携を再利用する。
- 効果システム: Koka 風の行多相な効果型。ハンドラは evidence passing でコンパイルし、末尾再開形 (tail-resumptive) のハンドラは通常の関数呼び出しと同じコストにする。一般のハンドラ(継続の捕捉)は Perceus と相性の良い方式 (Koka の実装) を参考にする。ケイパビリティはその特殊例。
- セッション型: 言語組み込みにし、双対性の自動検査を行う。

## 10. 性能への影響のまとめ

| 機能 | 実行時 | コンパイル時 |
|------|--------|--------------|
| 洗練型・契約 (`verify`) | なし (消去)。証明済みの境界検査は削除されむしろ速くなる | `--verify` 時のみ SMT。結果はキャッシュ |
| 契約 (`check`) | debug ビルドのみアサーション | 小 |
| 終了性検査 | なし | 小 |
| 軽量依存型 | なし (型は消去)。`SNat` を渡す場合のみ整数1つ | 型レベルの正規化 |
| ケイパビリティ | 引数1つ分 (ゼロサイズ型として最適化で消去) | なし |
| セッション型 | なし (チャネルの状態は型のみ) | なし |
