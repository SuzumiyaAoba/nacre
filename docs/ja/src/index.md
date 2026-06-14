# Nacre

> Read in English: [Nacre Documentation](../index.html)

<img class="nacre-cover" src="../assets/nacre-compiler.png" alt="構造化された Nacre ソースがコンパイラを通ってシェルスクリプトへ変換される図" />

<p class="nacre-lede">
Nacre は、単体で実行できる Bash スクリプトへコンパイルされる、型付きの
スクリプト指向言語です。実行ファイルやファイルシステムへの権限をソースコード
自身には持たせず、別途レビューするポリシーで管理します。
</p>

## 目的から選ぶ

<div class="nacre-links">
  <a href="tutorial.html">最初のプログラムを作る</a>
  <a href="language-reference.html">言語仕様を調べる</a>
  <a href="examples.html">検証済みサンプルを見る</a>
  <a href="security-policy.html">セキュリティモデルを理解する</a>
</div>

## 小さな例

```nacre
fn greet(name: String): String {
    return "こんにちは、${name}"
}

const message = greet("Nacre")
run.output.echo(message)
```

このソースコードは、実行ファイルを直接選びません。ポリシーが
`run.output.echo` とレビュー済みの実行ファイルを対応付けます。

```toml
[command_groups.output.commands.echo]
program = "bin/echo"
```

次のようにコンパイルします。

```bash
cargo run -- --policy policy.toml input.ncr output.sh
bash output.sh
```

## 実装済みの範囲

現在のコンパイラは、次の機能に対応しています。

- 静的型検査を伴う不変・可変バインディング
- プリミティブ値、Option、Result、配列、マップ、タプル、レコード
- 関数、ジェネリクス、トレイト、ラムダ、新しい型、直和型
- `if`、`while`、`for`、網羅性を検査する `match` 式
- モジュールと同梱の `std` モジュール
- ポリシーで許可したコマンドと、保護されたファイル操作
- Rust ライブラリ API とコマンドラインコンパイラ

任意のシェルコマンド、パイプライン、リダイレクト、生の Bash、
バックグラウンドコマンド、`require` 文は安全プロファイルで拒否されます。
その他の制約は[現在の制限事項](limitations.md)を参照してください。

## ドキュメントの検証

[検証済みサンプル](examples.md)に掲載したプログラムは、すべて
`scripts/verify-docs.sh` でコンパイル・実行されます。英語版と日本語版は
別々にビルドしたうえで言語対応検索の同じインデックスへ収録されるため、それぞれに
正しいナビゲーション、検索結果、文書言語が設定されます。

このリポジトリのコードは、すべて Coding Agent によって開発されました。
詳細は[開発者表記](development-attribution.md)を参照してください。
