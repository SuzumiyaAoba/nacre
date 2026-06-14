# 検証済みサンプル

このページのプログラムは、すべて `scripts/verify-docs.sh` でコンパイル・実行
されます。各サンプルは共通の [`policy.toml`](../examples/policy.toml) を使います。

## Hello

```nacre
{{#include ../../examples/hello.ncr}}
```

[ソースを開く](../examples/hello.ncr)

## 環境変数と算術演算

```nacre
{{#include ../../examples/env-and-math.ncr}}
```

[ソースを開く](../examples/env-and-math.ncr)

## 制御フロー

ネストしたブロックは、1階層につき4スペースで整形しています。

```nacre
{{#include ../../examples/control-flow.ncr}}
```

[ソースを開く](../examples/control-flow.ncr)

## 関数

```nacre
{{#include ../../examples/functions.ncr}}
```

[ソースを開く](../examples/functions.ncr)

## 日本語テキスト

Nacre のソースと生成されるスクリプトは、UTF-8 の文字列を保持します。

```nacre
{{#include ../../examples/japanese.ncr}}
```

[ソースを開く](../examples/japanese.ncr)

## 許可されたコマンド

```nacre
{{#include ../../examples/commands.ncr}}
```

[ソースを開く](../examples/commands.ncr)

## 保護されたファイル操作

```nacre
{{#include ../../examples/filesystem.ncr}}
```

[ソースを開く](../examples/filesystem.ncr)

## 共通ポリシー

```toml
{{#include ../../examples/policy.toml}}
```

[ポリシーを開く](../examples/policy.toml)
