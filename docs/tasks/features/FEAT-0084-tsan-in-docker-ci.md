# FEAT-0084: Docker の CI で ThreadSanitizer を動かす

- 状態: in-progress
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0045
- 設計: [05-runtime.md](../../design/05-runtime.md) §9.7, [10-testing.md](../../design/10-testing.md)

## 概要

FEAT-0045 の並行性のテスト (`tsan: 1` の MIR 実行テスト) は、ThreadSanitizer が動く環境でだけ TSan の構成を実行する。
Docker のコンテナの中では TSan のプログラムが起動できないため、CI (`scripts/ci.sh`) は TSan の構成を飛ばしており、TSan の検査はホストで行っている。
コンテナの中でも TSan が動くようにし、CI で常に TSan の検査を行う。

## 原因

- Ubuntu 24.04 の既定では ASLR のエントロピーが高い (`vm.mmap_rnd_bits=32`)。LLVM 18 の TSan はこの配置を扱えず、
  `personality(2)` で `ADDR_NO_RANDOMIZE` を立てて自分を再実行する。
- Docker の既定の seccomp プロファイルは `personality` を決まった引数 (`0`・`PER_LINUX32` など) でしか許さないため、
  `ThreadSanitizer: CHECK failed: tsan_platform_linux.cpp:282 "((personality(old_personality | ADDR_NO_RANDOMIZE))) != ((-1))"` で止まる。

## 対応の候補と決定

1. `compose.yaml` に、Docker の既定のプロファイルに `personality(ADDR_NO_RANDOMIZE)` だけを足した seccomp プロファイルを指定する (変更が最も小さい)。
2. `compose.yaml` で seccomp を無効にする (`security_opt: [seccomp=unconfined]`)。簡単だが、コンテナの制限が大きく緩む。
3. ホストの `vm.mmap_rnd_bits` を 28 に下げる (ホスト全体の設定を変える)。
4. 高エントロピーの ASLR を扱える版の LLVM の TSan が使えるか調べる (イメージの LLVM の版の更新を伴う)。

コンテナの権限やホストの設定に関わるため利用者の判断を待ち、2026-11-14 に利用者が **1** を選んだ。

## 作業内容

- [x] 利用者が対応を選ぶ (1)
- [ ] `docker/seccomp.json`: Docker 29.8.0 が組み込む既定のプロファイル (moby/profiles `seccomp/v0.2.3` の `default.json`、Apache-2.0) に
      `personality` の引数 `262144` (`PER_LINUX | ADDR_NO_RANDOMIZE`) を許す規則を 1 つ足したもの。由来と変更は `docker/README.md`
- [ ] `compose.yaml` の `dev` に `security_opt: [seccomp=docker/seccomp.json]`
- [ ] `scripts/ci.sh` の ThreadSanitizer の段: TSan が動かなければ (警告ではなく) 失敗にする
- [ ] コンテナの中で `scripts/ci.sh` の ThreadSanitizer の段が「動く」と表示し、`tsan: 1` のテストが通ることを確かめる
- [ ] 10-testing.md と 05-runtime.md §9.7 の「コンテナでは飛ばす」の記述を更新する

## 受け入れ条件

- [ ] Docker のコンテナの中で `tsan: 1` のテストが TSan の構成でも実行され、報告がない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- 2026-11-14: FEAT-0045 の作業中に起票。seccomp を緩める変更はコンテナの権限に関わるため、自動では入れず利用者の判断を待つ。
  ホスト (同じ LLVM 18、`vm.mmap_rnd_bits=32`) では TSan が動き、FEAT-0045 の TSan の検査はそこで行った。
