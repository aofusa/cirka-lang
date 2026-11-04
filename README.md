# CirkaLang / aCHC

**CirkaLang** は、証明系 (依存型・SMT による自動証明) と Haskell のような表現力を持ち、
Rust のように **トレーシング GC なしでメモリ安全かつ高速** な言語を目指すプロジェクトです。

第一段階では、共通のバックエンド (共通 Core → MIR → Perceus → LLVM、`libcirka_rt`) の上に
二つの試作を作っています。

| 試作 | コンパイラ | 拡張子 | 内容 |
|------|-----------|--------|------|
| aCHC (aofusa's CirkaHaskellCompiler) | `achc` | `.hs` / `.chs` | Haskell を GC なしで実行し、所有権拡張を試す |
| CirkaLang v0.0.x | `cirka` | `.cirka` | 正格・QTT 依存型・所有権・証明を持つ最終形のプロトタイプ |

設計は [docs/design/00-overview.md](docs/design/00-overview.md)、作業規約は [AGENTS.md](AGENTS.md)、
チケット一覧は [docs/tasks/tasks.md](docs/tasks/tasks.md) を参照してください。

## 開発環境 (Docker)

ビルドとテストは Ubuntu 24.04 の Docker コンテナで行います (LLVM 18・GHC・Rust・Z3・valgrind・hyperfine 入り)。

```sh
cp .env.sample .env            # 初回のみ。CIRKA_UID / CIRKA_GID を `id -u` / `id -g` に合わせる
docker compose build           # イメージ作成
docker compose run --rm dev    # コンテナ内シェル

# コンテナ内で
scripts/ci.sh                  # fmt / clippy / ランタイム / 全テスト
scripts/build-runtime.sh       # libcirka_rt.a (release / debug) のみ
scripts/memcheck.sh            # valgrind (CIRKA_ALLOC=system) で実行テスト
scripts/bench.sh               # ベンチマーク (hyperfine)
```

環境依存の値 (ツールのパス・版・遠隔ホストなど) はすべて `.env` に置きます。項目の説明は
[.env.sample](.env.sample) にあります。シェルからは `source scripts/env.sh` で読み込めます。

## ライセンス

MIT または Apache-2.0 のデュアルライセンスです。
