# 言語リファレンス

このページでは、現在実装されている安全プロファイルを説明します。より広い設計原則は
[言語設計](syntax.md)を参照してください。

## ソースファイル

Nacre のソースは UTF-8 テキストで、`.ncr` 拡張子を使います。通常は改行で文を
区切ります。空行、shebang、`##` で始まるコメントを記述できます。

```nacre
#!/usr/bin/env nacre

## `let` を指定しない値は不変です。
const project = "Nacre"
let revision = 1
revision = revision + 1
revision += 1
```

ブロックは波括弧で囲みます。インデントは構文上の意味を持ちませんが、この
ドキュメントでは1階層につき4スペースを標準とします。

## バインディングとプリミティブ型

`const` は不変、`let` は可変のバインディングを作成します。型を推論できる場合、
注釈は省略できます。

```nacre
const name: String = "Nacre"
let count: Int = 1
count = count + 1
count += 1

const enabled: Bool = true
const ratio: Float = 1.5
const path: Path = "input/data.txt"
const status: ExitCode = 0
const nothing: Unit = ()
```

明示的なキャストは、`String` と `Path` の変換、許可された数値の拡張、
newtype の変換に対応します。

型注釈では union 型と intersection 型を記述できます。

```nacre
const input: String | Path = "input.txt"
const pathLike: String & Path = "input.txt"
```

## 構造化された値

```nacre
const names: [String] = ["Ada", "Grace"]
const ports: Map[String, Int] = { "http": 80, "https": 443 }
const endpoint: (String, Int) = ("localhost", 8080)
const user: { name: String, age: Int } = {
    name: "Ada",
    age: 36
}
```

配列、タプル、レコードは分割代入できます。

```nacre
const (host, port) = endpoint
const { name, age } = user
const [first, second, ...rest] = names
const ([nestedFirst], [tag, ...tags]) = (["Ada"], ["compiler", "math"])
const issue = { owner: { id: 7 }, labels: ["bug", "runtime"] }
const { owner: { id }, labels: [primaryLabel, ...otherLabels] } = issue
```

コレクションでは `.len()`、`.isEmpty()`、`.map(...)`、
`.contains(...)`、`.slice(...)`、`.keys()`、`.values()`、
`.set(...)`、`.remove(...)` などを使用できます。利用できる操作は
レシーバーの型によって異なります。

## 文字列とパス

文字列では補間と三重引用符による複数行の値を使用できます。

```nacre
const name = "Nacre"
const greeting = "こんにちは、${name}"
const values = ["a", "b"]
const indexed = "first: ${values[0]}"
const count = "items: ${values.len() + 1}"
const message = """
1行目
2行目
"""
const literal = r"バックスラッシュ \n をそのまま保持"
```

文字列補間では、変数、配列 index、レコード field、タプル field、演算子、
メソッド呼び出しを含む式を使えます。
文字列と Path 値には、`.len()`、`.isEmpty()`、`.slice(...)`、
`.contains(...)`、`.trim()`、`.replace(...)`、`.basename()`、
`.dirname()`、`.stem()`、`.extname()` などの操作があります。

## 関数

```nacre
fn greet(name: String, prefix: String = "Hello"): String {
    return "${prefix}, ${name}"
}

fn firstLabel(prefix: String, values: ...String): String {
    return "${prefix}: ${values[0]}"
}

const defaultGreeting = greet("Nacre")
const customGreeting = greet("Nacre", "Hi")
const namedGreeting = greet(prefix = "Hi", name = "Nacre")
```

ジェネリック関数、トレイト境界、関数値、式ラムダ、UFCS 形式の呼び出しに
対応します。

```nacre
fn identity[T](value: T): T {
    return value
}

fn decorate(value: String): String {
    return "[${value}]"
}

const names = ["Ada", "Grace"]
const decorated = names.map(decorate)
const decoratedValue = ("Na" ++ "cre").decorate()
```

関数呼び出しでは、位置引数の後に名前付き引数を指定できます。名前付き引数の順序は
自由で、省略したパラメータにデフォルト値があればそれが使われます。

## Option と Result

Option は `T?` または `Option[T]`、Result は `Result[T, E]` または
`T \/ E` と記述します。

```nacre
const present: String? = Some("value")
const missing: String? = None
const fallback = missing.orElse(Some("fallback"))

const ok: Result[Int, String] = Ok(7)
const error: Result[Int, String] = Err("invalid")
const incremented = ok.map(value => value + 1)
```

型に応じて `.map(...)`、`.ap(...)`、`.flatMap(...)`、遅延評価される
`.orElse(...)` を使用できます。`do { ... }` 式では `<-` バインディングと、
文脈から型を決める `pure(...)` を利用できます。

## 制御フロー

```nacre
let count = 3

while count > 0 {
    count = count - 1
}

for name in ["Ada", "Grace"] {
    const length = name.len()
}

for index in 0..3 {
    const copy = index
}

for (name, score) in [("Ada", 10), ("Grace", 12)] {
    const label = "${name}: ${score}"
}

const label = if count == 0 {
    "done"
} else {
    "pending"
}
```

`if`、`else if`、`else`、`while`、`for`、`break`、`continue` に
対応します。`for` は配列と整数 range を反復できます。`start..end` は終端を含まず、
`start..=end` は終端を含みます。降順の range にも対応します。単独のブロックは
静的なスコープを作ります。ループ束縛では、配列内のタプル、レコード、配列要素を
nested binding pattern や配列 rest 束縛を含めて destructure できます。`break` と
`continue` はループ内でのみ使用でき、無条件に制御が移った後の文は到達不能として
拒否されます。

`defer` は、現在のブロックを抜けるときに実行する式文またはブロックを登録します。
defer 文は通常のスコープ終了、`return`、`break`、`continue` の直前に
後入れ先出しで実行されます。defer 本体から制御フローを抜けることはできません。

`Unit` 以外を返す関数では、すべての経路が値を返す必要があります。関数末尾の
式は暗黙の return となり、それ以前の分岐が明示的に return する場合も同様です。

## パターンマッチ

`match` は、リテラル、ワイルドカード、タプル、レコード、配列/rest、Option、
Result、直和型のパターンに対応します。`pattern as name` は、内側のパターンに
一致した後で match 対象全体を束縛します。閉じた型ではチェッカーが網羅性を検査します。

```nacre
type Message = Text(String) | Pair(Int, Int) | Empty

fn describe(message: Message): String {
    return match message {
        Text(text) if !text.isEmpty() => text,
        Pair(left, right) => "${left}:${right}",
        Empty => "empty",
        _ => "blank"
    }
}

const values = ["a", "b", "c"]
const summary = match values {
    [first, ...rest] as all => "${first}:${rest[1]}:${all.len()}",
    _ => "empty"
}
```

## 型、トレイト、モジュール

```nacre
type Identifier = Int
newtype UserId = Int
newtype Id[T] = String
type Box[T] = Box(T)
type Tree[T] = Leaf(T) | Branch(Tree[T], Tree[T])

impl UserId {
    const prefix = "usr_"

    fn parse(value: String): UserId {
        return UserId(value)
    }

    fn display(value: UserId): String {
        return "${UserId.prefix}${value.value}"
    }
}

trait Show[T] {
    fn show(value: T): String
}

impl Show[Int] {
    fn show(value: Int): String {
        return "Int(${value})"
    }
}
```

`use` でモジュールを読み込みます。

```nacre
use std.path

const file: Path = "/tmp/archive.tar.gz"
const extension = path.extname(file)
```

読み込んだ宣言には名前空間が付きます。通常のモジュールは、読み込むファイルからの
相対パスで解決されます。同梱モジュールには `std.cli`、`std.fs`、`std.io`、
`std.json`、`std.log`、`std.path`、`std.process`、`std.str`、`std.test` が
あります。

入力ファイルから上位ディレクトリへ `nacre.toml` を探索し、見つかった場合は
ローカル path 依存を解決します。

```toml
[package]
name = "app"
version = "0.1.0"

[dependencies.tools]
path = "../tools"
```

```nacre
use tools.format

const label = format.label("nacre")
```

`as` を使うと、短いローカル名前空間でモジュールを import できます。

```nacre
use tools.format as fmt

const shortLabel = fmt.label("nacre")
```

依存名は `use` の先頭セグメントになります。`use tools.format` は
`../tools/format.ncr`、`../tools/format.d.ncr`、または
`../tools/format/index.ncr` を探します。`use tools` は依存ルートの
`index.ncr` を探します。相対 import と `std.*` は従来通り利用できます。
`nacre --write-lock input.ncr` は manifest と同じディレクトリに `nacre.lock` を
生成します。lockfile が存在する場合、path 依存の解決先と内容 fingerprint が
コンパイル時に検証されます。公開 registry 依存はまだありません。

## 環境変数と引数

```nacre
const shell = env.SHELL ?? "/bin/sh"
const home = process.env("HOME")
const arguments: [String] = args
```

環境変数とコマンドライン引数は、信頼できないデータとして扱ってください。
許可されたコマンドへ渡す場合も、個別の引数として保持されます。環境変数名は
実行ポリシーに列挙する必要があり、`process.env(...)` は静的な文字列リテラルの
名前だけを受け付けます。コマンドライン引数には、実行ポリシーの
`[process] args = true` が必要です。

## 許可されたコマンド

```nacre
const version = run.inspect.version()
run.output.echo("version: ${version}")

const inspected: CommandOutput = run.result.inspect.version()
const status: ExitCode = inspected.status
const stderr: String = inspected.stderr
```

名前は静的な `run.<group>.<command>` 形式でなければなりません。コンパイラは
[実行ポリシー](security-policy.md)を通して名前を解決します。コマンドは
標準出力を `String` として返します。失敗を値として扱いたい場合は
`run.result.<group>.<command>` を使います。この形式は
`CommandOutput` レコードを返し、`stdout: String`、`stderr: String`、
`status: ExitCode`、`success: Bool` を読み取れます。

## 演算子

実装済みの演算子は次のとおりです。

- 算術: `+`、`-`、`*`、`/`、`%`
- 連結: `++`
- 比較: `==`、`!=`、`<`、`<=`、`>`、`>=`
- 論理: `!`、`&&`、`||`
- ビット演算: `&`、`|`、`^`、`~`、`<<`、`>>`
- Applicative / Monad の別名: `<$>`、`<*>`、`>>=`、`<|`
- 既定値の取り出し: `??`

可変バインディングでは、`+=`、`-=`、`*=`、`/=`、`%=`、`++=`、
`&=`、`|=`、`^=`、`<<=`、`>>=` の複合代入も使用できます。

丸括弧で評価順序を指定できます。

## 拒否される構文

安全プロファイルでは、次の構文を拒否します。

- `$sh"..."`、`$sh'...'`、`$sh{ ... }`
- 生の Bash ブロック
- シェルのパイプラインとリダイレクト
- バックグラウンド、非同期、spawn 形式のシェルコマンド
- `hasCommand(...)`、`require(...)`、`requireOneOf(...)`

代わりに、用途を限定したレビュー済みの実行ファイルをポリシーへ追加してください。
