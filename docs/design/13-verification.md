# 13. 検証機能: 仕様と正しさの静的保証

入力: [docs/specs/03-dependent-types.md](../specs/03-dependent-types.md)、[docs/specs/04-features.md](../specs/04-features.md)

性能・GC なしのメモリ安全 (specs 00/01) を最優先とし、その上に載せる **二番目の軸** の機能群。
プログラムが仕様を満たしていること、バグがないことをコンパイル時に確認する。
**まず CirkaLang プロトタイプ (v0.0.4 以降) で設計・評価し** (ADR-016, ADR-017, ADR-020)、その結果を aCHC の後期 (H5–H7) に Haskell の拡張として取り込む (ADR-027、[14-achc-advanced.md](14-achc-advanced.md))。

## 1. 基本原則

| 原則 | 内容 |
|------|------|
| **P1. 実行時コストゼロ** | 型・証明・述語はすべて多重度 0 で **消去** される。生成コードに残るのは、利用者が明示的に選んだ実行時検査 (`--contracts=check`) だけ |
| **P2. 使わなければ影響なし** | 証明や述語を書かないコードは、通常の型検査だけで済む。SMT はコンパイル時にのみ、証明義務があるときだけ動く (結果はキャッシュ) |
| **P3. 自動化優先** | 手で証明を書くより、SMT ソルバ (Z3) による自動証明を優先する。証明の手書きは最後の手段 |
| **P4. 性能にはむしろ効く** | 証明できた事実 (添字が範囲内、除数が非ゼロ、分岐が到達不能) を使って実行時検査を **除去** する (ADR-019) |
| **P5. 信頼の範囲を明示** | `assume`/`believe_me`、FFI 宣言の述語、SMT ソルバ自体が信頼基盤。`cirka check --report` で仮定の一覧を出す (Lean の `sorry` 数に相当) |

## 2. 取り込む機能と時期

| 機能 | aCHC | CirkaLang プロトタイプ | CirkaLang v0.1.0〜 |
|------|------|------------------------|--------------------|
| **量的型理論 (0/1/ω)** | 多重度 `%0`/`%1`/`ω` (型の消去・線形性のみ) | ◎ v0.0.1 から型検査の基礎 | 同左 |
| **依存型** | ○ H5/H6: GADTs・型族・DataKinds と `foreach` (シングルトンへ脱糖) (FEAT-0140〜0144, 0150) | ◎ v0.0.1 から (QTT。最初は `Vect` 程度に限定)、v0.0.4 で証明に本格利用 | 同左 |
| **全域性・終了性** | ○ H6 (FEAT-0153) | ◎ v0.0.4 | 同左 |
| **等式証明 (`=`, `rewrite`)** | △ H6: `:~:` と反映 (reflection) | ◎ v0.0.4 | 同左 |
| **部分集合型 (洗練型) + SMT** | ○ H6: Liquid Haskell 互換注釈 + `.chs` の洗練型 (FEAT-0151) | ◎ v0.0.4 | 同左 |
| **契約 (requires/ensures/invariant)** | ○ H6 (FEAT-0152) | ○ v0.0.4 | 同左 |
| **証明による検査除去** | ○ H6 (FEAT-0154、共通基盤を共有) | ○ v0.0.4 | 同左 |
| **ケイパビリティ** | ○ H7 (FEAT-0160) | ○ v0.0.7 | 効果システムに統合 |
| **効果システム (Koka 風)** | △ H7: `Eff es a` モナド (FEAT-0161) | △ v0.0.7 で試作 | 採否を決定 |
| **セッション型** | △ H7 (FEAT-0163) | △ v0.0.7 (ライブラリ + 双対性検査) | 組み込みを検討 |
| **対話的証明 (タクティク)** | × | × | 様子見 (Lean への書き出しなどを検討) |

◎ 本格対応 / ○ 対応 / △ 限定・試作 / × 対応しない

**理由の要点**:

- 依存型と全域性は、型検査器の土台から作らないと後付けが難しい。CirkaLang は v0.0.1 から QTT の型検査器を持つので (ADR-023)、v0.0.4 で証明機能を足すのは型検査器の延長で済む。
- 一方で優先順位を守るため、証明を本格的に使うのは性能・メモリ安全の版 (v0.0.1–v0.0.3) の後とする。
- aCHC で依存型や検証を作ると Haskell の型システムの上限に縛られ、CirkaLang で作り直しになる。aCHC での検証は「既存の Haskell コードに注釈を書く」場合の参考実装にとどめる。

## 3. 依存型と証明 (CirkaLang v0.0.4)

### 3.1 全域性

```
total
length : List a -> Nat
length []        = 0
length (_ :: xs) = S (length xs)

-- 構造的再帰でない場合は尺度を示す
total
gcd : (a, b : Nat) -> Nat
gcd a 0 = a
gcd a b = gcd b (a `mod` b)  decreasing b
```

- 関数は `total` (網羅的かつ停止) / `covering` (網羅的) / `partial` を区別する。既定は `covering`。
- **型の中で使う関数・証明として使う関数は `total` 必須**。`partial` な関数の結果は証明に使えない。
- 停止性は構造的再帰と size-change 原理で自動判定し、判定できなければ `decreasing` で尺度を示す (E0502)。
- 確定した規則は [spec-v0.0.4.md](cirkalang/spec-v0.0.4.md) §1。全域性は推論され、`total` は検査の要求である。
  判定は型検査の後の TT に対して `cirkalang-check` が行い、必要な関数 (`total` と宣言した関数・型の中の関数) から辿れる関数だけを調べる。
  トレイトのメソッドは既知のインスタンスまで辿り、尺度の証明義務は Z3 で決める (`Int` / `Nat` の引数は尺度として自動で試す)。

### 3.2 等式と証明

```
appendNilRight : (xs : List a) -> xs ++ [] = xs
appendNilRight []        = Refl
appendNilRight (x :: xs) = rewrite appendNilRight xs in Refl
```

- 命題等価 `=`、`Refl`、`rewrite`、`with` 抽象、`Dec`、`So`、`Void` を `core` に置く。
- 証明は通常の関数として書き、多重度 0 で使えば実行時に消える。
- 確定した規則は [spec-v0.0.4.md](cirkalang/spec-v0.0.4.md) §2。`x = y` は Prelude の `Equal x y`、`rewrite` はプリミティブ `primRewrite` に展開され実行時には本体だけが残る。
  結果が等式か `Void` の関数は証明であり全域でなければならない (E0506)。証明の有無で `-O2` の MIR が同一であることをテストで確かめる。

### 3.3 部分集合型 (洗練型) と SMT

```
Pos : Type
Pos = {n : Int | n > 0}

safeDiv : Int -> {d : Int | d /= 0} -> Int

index : (xs : Array a) -> {i : Nat | i < length xs} -> a      -- 境界検査なしでコンパイルできる

-- 一意配列: 長さは書き込みで変わらない
write : (1 arr : Unique (Array a)) -> {i : Nat | i < length arr} -> a
     -> {r : Unique (Array a) | length r = length arr}
```

- `{x : A | p}` は「値 `x : A` と、**多重度 0 の証明** `p x`」の組 (Σ 型) として扱う。実行時は `A` と同じ表現。
- 値を部分集合型に渡すときの証明義務は、まず型検査器の正規化で試し、残りを **SMT (Z3) に問い合わせて自動的に解く**。
  述語の論理は SMT で決定可能な断片 (線形整数算術・真偽値・等号・未解釈関数・`total` な関数の展開) に限る。
- SMT で解けない場合は、利用者が証明項を書く (`{x : A | p}` に `(x ** proof)` を渡す)。
- 失敗時は反例 (Z3 のモデル) をソース上の変数名で表示する (E0501)。タイムアウトは警告 W0503 + 手動証明の案内。
- 所有権との統合: 一意な値には別名がないので、「消費前の値」と「返ってきた値」の関係を素直に述べられる (Flux / Verus と同じ発想)。
- 確定した規則は [spec-v0.0.4.md](cirkalang/spec-v0.0.4.md) §3。`{x : A | p}` は `A` に簡約される型 `subsetOf A p` で、Σ 型の組にはしない
  (変換が要らず、値の表現が同じ)。型検査器が照合の位置に印 `primRefine` を付け、`cirkalang-check` が経路の条件と仮定のもとで評価・Z3 で示す。
  手動の証明は、結果が `{u : () | P}` の全域な関数 (帰納法)・`So`・等式の証明を仮定として渡して行う。線形の型の部分集合型と高階の位置は v0.0.4 では扱わない。

### 3.4 契約

```
binarySearch : (arr : Ref r (Array Int)) -> (key : Int) -> Borrow r (Maybe Nat)
  requires sorted arr
  ensures  \res => case res of
             Just i  => i < length arr && arr ! i == key
             Nothing => not (elem key arr)

data Range : Type where
  MkRange : (lo, hi : Int) -> Range
  invariant lo <= hi
```

- `requires` は引数の部分集合型、`ensures` は結果の部分集合型、`invariant` は構築子の引数の部分集合型に脱糖する。
- 確定した規則は [spec-v0.0.4.md](cirkalang/spec-v0.0.4.md) §4。脱糖は構文解析で行い、`requires` は最後の明示的な引数、`invariant` は最後のフィールドを絞る。
  `check` は示せなかった義務の印 `primRefine` を `checkContract` (Prelude) に置き換えて実行時に検査する。
- `--contracts`:

| モード | 動作 | 実行時コスト |
|--------|------|--------------|
| `verify` (既定) | 静的に証明する (SMT + 手動)。証明できなければコンパイルエラー | なし |
| `check` | 静的に証明できなかった契約を実行時アサーションにし、コンパイルは通す (開発中・debug 向け) | 未証明の契約のみ |
| `off` | 契約を無視する (証明もしない) | なし |

### 3.5 証明による実行時検査の除去

- 配列の添字・除算・`unreachable` な分岐について、証明済みの事実を Core に注釈し、MIR で境界検査なしのアクセス・`unreachable` に変換する。
- 証明がない場合は従来どおり実行時検査が残る (安全側)。
- 確定した規則は [spec-v0.0.4.md](cirkalang/spec-v0.0.4.md) §5。証明に頼る演算は部分集合型の引数を持つライブラリ関数 (`Data.Array.index`・`divNZ`・`modNZ`・`unreachable`) の中だけにあり、
  `--contracts=off` ではそれらも検査のある演算に戻る。

## 4. ケイパビリティ・効果・セッション型 (CirkaLang v0.0.7)

### 4.1 ケイパビリティ

```
readFile : (1 cap : Cap FileSystem) -> Path -> IO (Cap FileSystem, Result IOError String)
```

- 能力トークンで、関数がどの種類の副作用 (ファイル・ネットワーク・時刻・乱数・プロセス) を使うかを型に表す。テストでは偽の能力を渡せる。
- 能力を取らない生の IO プリミティブは `std` の外に公開しない。
- 能力はゼロサイズ型として最適化で消える。

### 4.2 効果システム (試作)

- Koka 風の行多相な効果型 `a -> <io, exn | e> b`。ケイパビリティは効果の証拠 (evidence) として対応づける。
- ハンドラは evidence passing でコンパイルし、末尾再開形 (tail-resumptive) のハンドラは通常の関数呼び出しと同じコストにする。継続を捕捉する一般のハンドラは Koka の実装 (Perceus と相性が良い) を参考にする。
- 試作の結果で、v0.1.0 で効果システムを標準にするかを決める。

### 4.3 セッション型

```
Proto : Type
Proto = Send Int (Recv Bool End)

send  : (1 c : Chan (Send a p)) -> a -> IO (Chan p)
recv  : (1 c : Chan (Recv a p)) -> IO (a, Chan p)
close : (1 c : Chan End) -> IO ()
```

チャネルは `LType` なので、プロトコルの途中で捨てる・二重に使うことは型エラー。依存型でプロトコルを型レベルの値として書ける。双対性 `Dual p` は型レベル関数で計算する。

## 5. aCHC での検証 (後期 H6、FEAT-0151〜0154)

CirkaLang v0.0.4 で評価した検証機能を aCHC に取り込む (詳細は [14-achc-advanced.md](14-achc-advanced.md) §3)。
Liquid Haskell 互換の `{-@ @-}` 注釈 (Haskell としてはコメントなので `.hs` に書いても GHC でコンパイルできる) を共通基盤の `cirka-smt` で検証する。
TypeScript の `--checkJs` に相当する。`.chs` では洗練型・契約を構文として書ける (`.ts` の型注釈に相当)。

```haskell
{-@ safeDiv :: Int -> {d:Int | d /= 0} -> Int @-}
safeDiv :: Int -> Int -> Int
safeDiv n d = n `div` d
```

## 6. 実装の配置

| 部分 | クレート | 共有 |
|------|----------|------|
| SMT-LIB2 生成・Z3 呼び出し・キャッシュ・反例 | `cirka-smt` | 共通基盤 (両試作) |
| 全域性・終了性・証明義務の生成 | `cirkalang-check` | CirkaLang |
| 証明済み事実による検査除去 | `cirka-core` (注釈) / `cirka-mir` (変換) | 共通基盤 |
| Liquid 互換注釈・洗練型・契約 | `achc-*` (後期 H6) | aCHC |

外部ソルバのパスは `CIRKA_Z3`、タイムアウトは `CIRKA_VERIFY_TIMEOUT`。

## 7. 性能への影響のまとめ

| 機能 | 実行時 | コンパイル時 |
|------|--------|--------------|
| 依存型・証明・部分集合型 | なし (消去)。証明済みの検査は削除されむしろ速くなる | 型検査の正規化、証明義務がある場合のみ SMT (キャッシュ) |
| 契約 `verify` / `off` | なし | 同上 |
| 契約 `check` | 未証明の契約のみアサーション | 小 |
| 全域性検査 | なし | 小 |
| ケイパビリティ | なし (ゼロサイズ型) | なし |
| 効果型 (末尾再開形) | 関数呼び出しと同等 | 効果推論 |
| セッション型 | なし (状態は型のみ) | なし |
