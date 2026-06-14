# はじめに

このガイドでは、リポジトリのチェックアウトから小さな Nacre プログラムを
コンパイルして実行します。Nacre のソースファイルには `.ncr` 拡張子を使い、
Bash へコンパイルします。

## 前提環境

ローカルの Rust ツールチェイン、または Nix 開発環境を使用します。

```bash
nix develop path:.
```

リポジトリには、ビルド、テスト、ドキュメント生成に必要なツールが固定されています。

## 1. 純粋なプログラムを書く

`hello.ncr` を作成します。

```nacre
fn greet(name: String): String {
    return "こんにちは、${name}"
}

const names = ["Ada", "Grace"]
const message = greet(names.join(" と "))
```

コンパイルします。

```bash
cargo run -- hello.ncr hello.sh
bash hello.sh
```

純粋な計算にはポリシーが不要です。生成されるスクリプトは
`set -euo pipefail` で始まり、実行時にコンパイラを必要としません。

## 2. 許可されたコマンドを呼び出す

Nacre では、ソースファイルから実行ファイルを選べません。静的なコマンド呼び出しを
追加します。

```nacre
fn greet(name: String): String {
    return "こんにちは、${name}"
}

run.output.echo(greet("Nacre"))
```

`policy.toml` を作成します。

```toml
[command_groups.output.commands.echo]
program = "bin/echo"
```

実行ファイルのパスはポリシーファイルからの相対パスとして解決され、
正規化されます。コンパイルして実行します。

```bash
cargo run -- --policy policy.toml hello.ncr hello.sh
bash hello.sh
```

`run.output.echo(...)` へ渡した各式は、それぞれ1個の Bash 引数になります。
値に含まれるシェルのメタ文字は、実行可能な構文ではなくデータとして扱われます。

## 3. ファイルアクセスを許可する

ファイル操作にも外部ポリシーが必要です。

```toml
[filesystem]
read = ["workspace"]
write = ["workspace"]
```

ポリシーを読み込む時点で、指定したディレクトリが存在する必要があります。
プログラムから構造化された操作を使用できます。

```nacre
const output: Path = "workspace/message.txt"
fs.writeLines(output, ["1行目", "2行目"])

const lines = fs.readLines(output)
run.output.echo(lines.join(", "))
```

生成されたランタイムのガードは、設定したルート外のパスを拒否します。

## 4. コンパイルエラーを確認する

エラーにはソース行と具体的な理由が含まれます。

```nacre
const count: Int = "three"
```

```text
line 1: expected Int, found String
```

厳密な文言は変更される可能性がありますが、構文、型、ポリシー、ファイルに関する
エラーでは、CLI が0以外の終了ステータスを返します。

## 次に読むもの

- 式や宣言を調べるには[言語リファレンス](language-reference.md)
- 権限の境界を理解するには[実行ポリシー](security-policy.md)
- 動作するコードを見るには[検証済みサンプル](examples.md)
