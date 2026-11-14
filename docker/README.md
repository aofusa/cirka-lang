# docker/

開発用コンテナ (`compose.yaml`) の設定ファイル。

## seccomp.json

コンテナの seccomp プロファイル。Docker の既定のプロファイルに、規則を 1 つだけ足したもの (FEAT-0084)。

- 元: [moby/profiles](https://github.com/moby/profiles) の `seccomp/default.json`、タグ `seccomp/v0.2.3`
  (Docker 29.8.0 が組み込んでいる版。Apache License 2.0)。
- 変更: `personality` の引数 `262144` (`PER_LINUX | ADDR_NO_RANDOMIZE`) を許す規則を、既存の `personality` の規則の後に追加した。
  ほかは元のまま。

追加の理由: Ubuntu 24.04 の既定の高エントロピーの ASLR (`vm.mmap_rnd_bits=32`) では、ThreadSanitizer (LLVM 18) は
`personality(2)` で ASLR を切って自分を再実行する。既定のプロファイルはこの引数を許さないため、TSan のプログラムが起動できない
(05-runtime.md §9.7、10-testing.md)。ASLR を切るのは TSan で計装したプロセスだけで、ほかのプロセスには影響しない。

Docker を更新して既定のプロファイルが変わったら、新しい版の `default.json` に同じ規則を足して置き換える。
