# CLI リファレンス

`nacre` バイナリは、1個の `.ncr` ファイルを構文解析・型検査し、Bash へ
コンパイルします。

## 使用方法

```text
nacre [--policy policy.toml] <input.ncr> [output.sh]
```

リポジトリ内から実行する場合:

```bash
cargo run -- [--policy policy.toml] <input.ncr> [output.sh]
```

`--policy` を省略すると、すべてを拒否する実行ポリシーが使われます。純粋な
プログラムは通常どおりコンパイルできますが、コマンドとファイル操作は許可されません。

## 標準出力へ書き出す

出力パスを省略します。

```bash
cargo run -- hello.ncr
```

生成された Bash は標準出力へ、診断メッセージは標準エラー出力へ書き込まれます。

## ファイルへ書き出す

```bash
cargo run -- hello.ncr /tmp/hello.sh
bash /tmp/hello.sh
```

コンパイラは生成結果を書き込みますが、ファイルへ実行権限を付けません。
`bash` を通して実行するか、明示的に権限を設定してください。

## ポリシーを指定してコンパイルする

```bash
cargo run -- \
  --policy docs/examples/policy.toml \
  docs/examples/hello.ncr \
  /tmp/hello.sh
```

`--policy` は入出力パスより前に指定します。ポリシー内の実行ファイルと
ファイルシステムの相対パスは、ポリシーファイルのディレクトリから解決されます。
ソースモジュールは読み込むファイルのディレクトリから解決され、`std.*` は
同梱標準ライブラリを使用します。入力ファイルから上位へ `nacre.toml` を探索し、
`[dependencies.<name>] path = "..."` がある場合は、その manifest ディレクトリを
基準にローカル path 依存を解決します。

## 終了ステータス

コンパイルと任意のファイル書き込みに成功すると、CLI は正常終了します。
次の場合は0以外で終了します。

- 引数が不正
- ポリシーの読み込みまたは検証に失敗
- 入力ファイルの読み込みに失敗
- 構文、型、権限の検査に失敗
- 出力ファイルの書き込みに失敗

利用できる場合、エラーにはソース行が含まれます。

## ライブラリ API

Rust からは次の API を使用できます。

```rust
use nacre::{compile_file, compile_file_with_policy};
```

メモリ上のソースには `compile_source` と `compile_source_with_policy` を
使用します。すべて `Result<String, CompileError>` を返します。
