**C FFIを「Rust / Swift / cgo並みに簡単」にするための設計**を追加します。

この言語（仮称 **LinHask**）では、Cの世界を「所有権付きの線形資源」として型システムに自然に取り込みます。線形型／一意性型があるので、Cポインタを「安全に扱える」形で包むのが本質的にやりやすいです。

### 1. 基本的なFFI宣言（最も簡単な形）

```haskell
-- シンプルなexternブロック（Rust風）
extern "C" {
  -- 関数
  c_malloc   :: CSize -> IO (Ptr ())
  c_free     :: Ptr () %1 -> IO ()
  c_printf   :: CString -> ... -> IO CInt   -- 可変長引数もサポート

  -- 構造体（opaqueでも可）
  type CFile
  fopen  :: CString -> CString -> IO (Ptr CFile)
  fclose :: Ptr CFile %1 -> IO CInt
  fread  :: Ptr a -> CSize -> CSize -> Ptr CFile -> IO CSize
}

-- またはSwift風に「モジュールインポート」
import "C" libm
-- これだけでsin, cosなどが使えるようにする（後述の自動生成）
```

- デフォルトで`unsafe`ではなく、「線形ポインタ」として扱うことを推奨。
- `Ptr a` 自体を線形型（`Ptr a %1`）にするか、`OwnedPtr a` という専用型を用意する。

### 2. 所有権を型で表現する安全なラッパー

Cから来たポインタは「所有権を持つ」か「借用」かを型で区別します。

```haskell
-- 所有権付きポインタ（最後に必ず解放される）
data OwnedPtr a

-- 借用（読み取り専用／一時的）
data BorrowedPtr a

-- コンパイラが自動でDropを挿入
instance Drop (OwnedPtr a) where
  drop p = c_free (cast p)   -- またはユーザー定義の解放関数

-- 安全なAPI例
withFile :: FilePath -> (BorrowedPtr CFile -> IO a) -> IO a
withFile path k = do
  owned <- fopen (toCString path) "r"
  withOwned owned k   -- 借用を渡し、スコープ終了で自動fclose
```

- **RAIIが自然に効く**：`OwnedPtr` を線形で持つ限り、使い忘れたらコンパイルエラー、使い終わったら自動解放。
- 例外・早期returnでも解放が保証される（線形型の性質）。

### 3. 簡単な取り込み方（3段階）

| レベル | 方法 | 手間 | 安全性 | 例 |
|--------|------|------|--------|----|
| **最速** | 自動バインディング生成 | ほぼゼロ | 中 | `import "C" libcurl` で主要関数が使える |
| **標準** | 手動`extern` + 薄いラッパー | 低 | 高 | 上記の`OwnedPtr`で包む |
| **完全制御** | 手動で線形型を細かく指定 | 中 | 最高 | 状態機械を型で表現（例：ソケットの状態） |

**自動生成のイメージ**（cgoやSwiftの`import C`に近い）：
- Cヘッダをパース（またはclangのASTを利用）
- 関数・構造体・定数を自動で`extern`宣言に変換
- ポインタ引数・戻り値にデフォルトで`OwnedPtr` / `BorrowedPtr` を割り当て
- ユーザーは必要に応じて上書き

### 4. 具体的な使いやすさの工夫

```haskell
-- 1. 文字列の変換を楽に
toCString :: String -> OwnedPtr CChar   -- 自動でnull終端 + 所有権
fromCString :: BorrowedPtr CChar -> String

-- 2. エラー処理も線形で
-- Cのerrnoや戻り値エラーをResultに自動変換するオプション
fopenSafe :: CString -> CString -> IO (Result (OwnedPtr CFile) Errno)

-- 3. コールバック（CからHaskell関数を呼ぶ）
-- 線形クロージャをC関数ポインタに変換
type CCallback = FunPtr (CInt -> CInt)
makeCallback :: (CInt %1 -> CInt) -> IO CCallback
```

- **null安全性**：`Ptr a` ではなく `Maybe (OwnedPtr a)` や専用の`Nullable`型を推奨。
- **アラインメント・パッキング**：構造体レイアウトをCと完全一致させる属性を用意。
- **スレッド安全性**：Cの呼び出しを「線形なWorld」や明示的な`unsafe`境界で制御可能。

### 5. ランタイム・コンパイル時の実装方針

- **LLVM経由**でコンパイルするので、Cの呼び出し規約（cdecl, stdcallなど）を直接扱える。
- ランタイムは最小限：
  - 文字列変換用の小さなヘルパー
  - `FunPtr`の管理（必要なら）
  - それ以外はゼロコストに近い
- 生成されるコードは「Cを直接呼んでいる」状態に近く、オーバーヘッドはほぼゼロ。

### 6. 他言語との比較での位置づけ

- **Rustより簡単**：`unsafe`ブロックを減らせる（線形型が安全性を保証するため）。所有権の移動が型で強制される。
- **Swift並みにシームレス**：`import "C" libfoo` で多くの場合すぐ使える。
- **cgoより安全**：ポインタの所有権と解放が型で保証される。メモリリークや二重解放が起きにくい。
- **純粋性を保つ**：C呼び出し自体は`IO`や線形`World`の中に閉じ込める。純粋な関数からは直接呼べない。

### まとめ

この追加要件は、線形型／一意性型を持つ言語にとってむしろ**強み**になります。  
Cのポインタを「線形な所有権付き資源」として扱うことで、

- 宣言は簡単（`extern`や自動import）
- 安全性は型で保証（忘れずに解放、二重解放なし）
- パフォーマンスはほぼゼロオーバーヘッド

を同時に達成できます。

必要であれば、次のような具体例も設計できます：

- libcurlやOpenSSLなどの現実的なバインディング例
- コールバックの扱い
- C構造体のメモリレイアウト制御
- エラーコードの自動変換規則

どのあたりを詳しく見たいか教えてください。
