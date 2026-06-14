# テストとカバレッジ

リポジトリには、コンパイラの動作、ドキュメント、カバレッジを個別に検証する
ゲートがあります。

## コンパイラの検証

```bash
nix develop path:. -c scripts/check.sh
```

次の処理を実行します。

1. `cargo fmt -- --check`
2. すべてのターゲットと機能に対し、警告をエラーとして Clippy を実行
3. すべてのターゲットのテストを実行

テストは、構文解析、型検査、Bash 出力、公開 API、CLI、ポリシー検証、
実行時パスガード、モジュール、代表的な言語プログラムを対象とします。

## ドキュメントの検証

```bash
nix develop path:. -c scripts/verify-docs.sh
```

スクリプトは次の処理を行います。

1. `docs/examples/` 以下のすべての `.ncr` ファイルをコンパイル
2. サンプル用ポリシーで生成された Bash を実行
3. 英語版と日本語版のドキュメントをビルド
4. 言語対応の Pagefind インデックスを生成
5. 必須ページ、共有アセット、言語メタデータ、リンク、コードの整形を検査
6. 英語と日本語の代表的な検索語を使ってインデックスを実行検証

生成されたサイトは `site/` へ出力されます。

サンプルを実行せずにドキュメントだけをビルドする場合:

```bash
nix develop path:. -c scripts/build-docs.sh
```

## カバレッジゲート

```bash
nix develop path:. -c scripts/coverage.sh
```

既定の非回帰しきい値は次のとおりです。

- 行カバレッジ 48%
- 関数カバレッジ 66%

ローカルでは `COVERAGE_MIN_LINES` と `COVERAGE_MIN_FUNCTIONS` で
上書きできます。

## ツールチェイン

`flake.lock` が Nix の入力を固定します。`rust-toolchain.toml` は、
Rustfmt、Clippy、LLVM ツールを含む同等の rustup 構成を提供します。
Nix 開発環境には、ドキュメント検証用の mdBook、Node.js、Pagefind Extended も
含まれます。

GitHub Actions は、GitHub Pages を公開する前に同じ二言語サイトをビルドします。
