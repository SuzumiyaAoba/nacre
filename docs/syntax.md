# Nacre — Syntax Design

> シェルスクリプトへトランスパイルする静的型付き言語

## 設計方針

1. **軽量な構文** — 冗長なボイラープレートを排除し、書きやすく読みやすい構文を目指す
2. **シェルスクリプトとの対応が見える** — 生成されるシェルスクリプトを想像できる構文設計
3. **静的型付け + 型推論** — 安全性を確保しつつ、型注釈の負担を最小化
4. **パイプ・リダイレクトは再設計** — シェルの `|` `>` 構文をそのまま引き継がず、言語として自然な形で表現
5. **ポータビリティの最大化** — 生成される Bash スクリプトは可能な限り単一ファイルで完結（Self-contained）し、外部の依存ファイルなしで動作することを目指す

---

## 型システム

### プリミティブ型

```text
String       ## 文字列（シェル上はすべて文字列だが、型として区別する）
Int          ## 整数
Float        ## 浮動小数点数
Bool         ## 真偽値
Path         ## ファイルパス（String のサブタイプ的に振る舞う）
ExitCode     ## 終了コード（0-255）
Unit         ## 戻り値なし（リテラル: `()`）
```

### リテラル

| 型 | リテラル例 | 備考 |
|---|---|---|
| `Unit` | `()` | 値を持たない |
| `Bool` | `true`, `false` | |
| `Int` | `42`, `-1`, `0xFF` (16進数), `0b1010` (2進数) | |
| `Float` | `3.14`, `-0.5` | |
| `String` | `"hello"`, `'world'` | `${}` による展開が可能 |
| `String` | `r"raw content"` | `${}` 展開を行わない生文字列 |
| `T?` | `Some(x)`, `None` | `Option` 型 |
| `T \/ E` | `Ok(x)`, `Err(e)` | `Result` 型 |
| `[T]` | `["a", "b"]` | 配列 |
| `Map[K, V]` | `{ "key": "value" }` | 連想配列 |

---

## 組み込みオブジェクト

### `env` (Environment Variables)

プロセスの環境変数にアクセスするための組み込みオブジェクト。

```nacre
const home = env.HOME              ## String? 型（未定義なら None）
const path = env.PATH ?? "/usr/bin" ## デフォルト値付き
```

**トランスパイル結果:**
`${NAME}` などのシェル変数参照に直接変換される。

### 組み込み関数

#### `hasCommand(cmd: String): Bool`

コマンドがシステムに存在するかどうかを確認し、真偽値を返す。`require` と異なり、コマンドが存在しなくてもスクリプトは終了しない。

```nacre
if hasCommand("fzf") {
  ## fzf があればリッチな UI を使う
} else {
  ## なければシンプルな fallback
}
```

#### `require(cmd: String, version: String? = None): Unit`

指定されたコマンドの存在（および必要に応じてバージョン）を確認する。存在しない場合はエラーメッセージを表示してスクリプトを終了する。

```nacre
require("git", version = ">= 2.25")
require("jq")
```

#### `requireOneOf(cmds: [String]): Unit`

リストされたコマンドのうち、少なくとも一つがシステムに存在することを確認する。一つも存在しない場合はスクリプトを終了する。

```nacre
requireOneOf(["curl", "wget"])
```

---

### 組み込み型

```text
CmdError     ## コマンドエラー（code: ExitCode, stderr: String）
```

### コンポジット型

```text
[T]             ## 配列
Map[K, V]       ## 連想配列（Dictionary）
T?              ## Option（Some(T) | None）
T \/ E          ## Result（Ok(T) | Err(E)）
A | B           ## ユニオン型（直和型）
A & B           ## インターセクション型（交差型）
{ ... }         ## レコード（構造体的な型）
(A, B, ...)     ## タプル
A => B          ## 関数
(A, B) => C     ## 複数引数の関数
```

### 型注釈

型推論があるため、多くの場合省略可能。

```nacre
const name = "hello"        ## String と推論
let count: Int = 42         ## 明示的な型注釈
const pi = 3.14             ## Float と推論
```

---

## 変数

### 再代入不可（`const`）

```nacre
const name = "world"
const greeting = "Hello, ${name}"
```

**トランスパイル結果:**

```bash
readonly name='world'
readonly greeting="Hello, ${name}"
```

### 再代入可能（`let`）

```nacre
let count = 0
count = count + 1
```

**トランスパイル結果:**

```bash
count=0
count=$((count + 1))
```

### 環境変数の参照

```nacre
const home = env.HOME              ## String? 型
const path = env.PATH ?? "/usr/bin" ## デフォルト値付き
```

**トランスパイル結果:**

```bash
readonly home="${HOME}"
readonly path="${PATH:-/usr/bin}"
```

### 分解代入（Destructuring）

配列、タプル、レコードの要素を変数に展開して束縛できる。

```nacre
const [first, ...rest] = files
const (host, port) = ("localhost", 8080)
const { name, age } = user
```

**トランスパイル結果:**

シェルスクリプトの変数参照や配列のスライスに展開される。

```bash
first="${files[0]}"
rest=("${files[@]:1}")
host="localhost"
port=8080
name="$user_name"
age="$user_age"
```

### スコープ

ブロック `{ ... }` は新しいスコープを形成する。Nacre では、上位スコープと同じ名前の変数を再定義すること（シャドウイング）は**禁止**されている。

```nacre
const x = 10
{
  const x = 20    ## コンパイルエラー: x はすでに定義されている
}
```

### 特殊変数 `_` (Discard)

値を使用しないことを明示するために `_` を使用できる。

```nacre
const _ = try $sh"cmd"      ## 戻り値を破棄
const [first, _] = list  ## 二番目の要素を無視
```

---

## 文字列

### 文字列リテラル

文字列はダブルクォート `"` とシングルクォート `'` の両方で囲むことができます。どちらも `${...}` による式展開をサポートします。
内包する文字列に応じて使い分けることで、エスケープ（`\"` や `\'`）を減らすことができます。

```nacre
const double = "He said 'Hello', ${name}"
const single = 'He said "Hello", ${name}'
```

### 生文字列 (Raw Strings)

`r` プレフィックスを付けることで、`${...}` による式展開を無効化できます。正規表現や他の言語のコードを埋め込む際に便利です。

```nacre
const regex = r"^\d+-\d+$"
const bash = r'echo "${BASH_VERSION}"'
```

### 複数行文字列

```nacre
const text = """
  This is a
  multi-line string
"""
```

---

## 演算子

- **算術演算子**: `+`, `-`, `*`, `/`, `%`
    - `Float` 型の演算は、トランスパイラによって `bc` や `awk` を用いた浮動小数点演算コードへ自動的に展開される。
- **ビット演算子**: `&`, `|`, `^`, `~`, `<<`, `>>`
    - Bash の `$(( ))` 内部でのビット演算に展開される。
- **比較演算子**: `==`, `!=`, `<`, `<=`, `>`, `>=`
- **論理演算子**: `&&`, `||`, `!`
- **型キャスト**: `as`（例: `value as Celsius`）
- **文字列結合**: 式展開（`"Hello, ${name}"`）を推奨するが、明示的な結合演算子 `++` も利用可能。

---

## コマンド実行

コマンド実行は言語の中核機能。
**コマンドは常に失敗するかもしれない計算として扱う。**

コマンド実行はすべて `$sh"..."` で行う。`$sh"..."` は `String \/ CmdError` 型を返す。

- `?` — `T \/ E` を `T?`（Option）に変換する（エラーを `None` に丸める）
- `try` / postfix `!` — `T \/ E` から `T` を取り出す（失敗時は早期リターン／大域脱出）
- `??` — `T?` または `T \/ E` にデフォルト値を与える

### コマンド内での変数展開と安全性

`$sh"..."` 内で `${var}` のように変数を展開した場合、トランスパイラは自動的に変数を安全に展開（エスケープ）し、シェルインジェクションの脆弱性を防ぐ。マクロ的な単なる文字列置換ではなく、安全なシェル変数参照として出力される。


### 基本的なコマンド呼び出し
```nacre
$sh'echo "Hello, world"'              ## String \/ CmdError 型
$sh'echo "Hello, world"'!             ## 成功を要求（失敗時は早期リターン）
$sh'echo "Hello, world"'?             ## String?（失敗したら None）
```

**トランスパイル結果:**

```bash
echo "Hello, world"
echo "Hello, world" || return $?
echo "Hello, world"               # Optionの場合、終了コードは無視
```

### コマンドの出力をキャプチャ

```nacre
const result = $sh"hostname"          ## String \/ CmdError 型
const name = try $sh"hostname"        ## String 型（失敗時は早期リターン）
const name2 = $sh"hostname"?          ## String?（失敗したら None）
const name3 = $sh"hostname"? ?? "x"   ## String（失敗時はデフォルト値）
```

**トランスパイル結果:**

```bash
result="$(hostname)"                # 終了コードは未チェック
name="$(hostname)" || return $?     # 失敗時に早期リターン
name2="$(hostname)"                 # 失敗しても続行（None）
name3="$(hostname)" || name3="x"    # 失敗時にデフォルト値
```

### 出力不要のコマンド実行

値を束縛しない `try (...)` は、トランスパイラが出力キャプチャを省略する。

```nacre
try $sh"mkdir -p \"build\""             ## 出力不要、成功を要求
try $sh"rm -rf \"tmp\""                 ## 出力不要、成功を要求
```

**トランスパイル結果:**

```bash
mkdir -p "build" || return $?       # 出力キャプチャなしに最適化
rm -rf "tmp" || return $?
```

### `match` でエラーをハンドリング

`Err` 側で `CmdError` の `code` と `stderr` にアクセスできる。
stderr へのアクセスがある場合、トランスパイラは自動的に stderr をキャプチャするコードを生成する。

```nacre
match $sh'curl -s "https://example.com"' {
  Ok(body) => try $sh"echo body",
  Err(e) => {
    try $sh'echo "Exit code: ${e.code}"'
    try $sh'echo "Error output: ${e.stderr}"'
  },
}
```

**トランスパイル結果:**

```bash
__stderr_tmp=$(mktemp)
if __stdout="$(curl -s "https://example.com" 2>"$__stderr_tmp")"; then
  echo "$__stdout"
else
  __code=$?
  __stderr="$(cat "$__stderr_tmp")"
  echo "Exit code: ${__code}"
  echo "Error output: ${__stderr}"
fi
rm -f "$__stderr_tmp"
```

### パイプライン

パイプは演算子 `|>` を使用する。パイプライン全体でひとつの `String \/ CmdError` を返す。

```nacre
const result = $sh'cat "file.txt"' |> $sh'grep "pattern"' |> $sh"sort" |> $sh"uniq -c"
## result: String \/ CmdError

const sorted = try ($sh'cat "file.txt"' |> $sh"sort")
## sorted: String（失敗時は早期リターン）
```

また、`|>` はコマンドだけでなく、Nacre の通常の関数に対しても使用できる。`x |> f` は `f(x)` と等価である。

```nacre
const names = ["alice", "bob", "charlie"]
const result = names
  |> (list => list.map(s => s.toUpper()))
  |> (list => list.join(", "))
## result: "ALICE, BOB, CHARLIE"
```

また、`String` をパイプでコマンドに流し込むこともできる。これは Bash の `<<<`（Here String）に相当する。

```nacre
try ("input data" |> $sh"grep input")
```

**トランスパイル結果:**
コマンド同士のパイプはシェルの `|` に、関数へのパイプは関数の入れ子構造（または中間変数の生成）に展開される。

#### パイプラインの安全性 (`pipefail`)
Nacre のパイプラインは、常に Bash の `set -o pipefail` 相当の挙動を持つ。パイプラインの途中のコマンドが一つでも失敗（終了コードが 0 以外）した場合、パイプライン全体の結果として `Err(CmdError)` を返す。

**トランスパイル結果:**

```bash
result="$(cat "file.txt" | grep "pattern" | sort | uniq -c)"
sorted="$(cat "file.txt" | sort)" || return $?
```

### リダイレクト

リダイレクト用の関数を提供:

```nacre
$sh'echo "hello"' >> write("output.txt")
$sh'echo "error"' >> append("log.txt")
$sh"cmd" >> write("out.txt", stderr = "err.txt")
```

**トランスパイル結果:**

```bash
echo "hello" > output.txt
echo "error" >> log.txt
cmd > out.txt 2> err.txt
```

### 非同期実行（Future / async / await）

コマンドをバックグラウンドプロセスとして実行し、非同期に処理するために `async` ブロックと `await` キーワードを使用する。
`async` ブロックは `Future[T]` 型を返す。

```nacre
## バックグラウンドでコマンドを実行（シェル上は `&` を付与して実行）
const f1 = async $sh"curl -s https://api.example.com/data1"
const f2 = spawn $sh"curl -s https://api.example.com/data2"

## 別の作業を並行して実行可能
try $sh'echo "Fetching data..."'

## 結果を待機する（シェル上は `wait` コマンドを使用）
const res1 = await f1
const res2 = f2.wait()
```

**トランスパイル結果:**

トランスパイラは一時ファイルや名前付きパイプ等を用いてバックグラウンドプロセスの出力をキャプチャし、`wait` によって同期するシェルスクリプトを出力する。

```bash
__future_out1=$(mktemp)
curl -s "https://api.example.com/data1" > "$__future_out1" 2>&1 &
f1_pid=$!

__future_out2=$(mktemp)
curl -s "https://api.example.com/data2" > "$__future_out2" 2>&1 &
f2_pid=$!

echo "Fetching data..." || return $?

wait $f1_pid
__code1=$?
res1="$(cat "$__future_out1")"
rm -f "$__future_out1"
# ※ 実際には終了コードをチェックし、失敗時は同じ終了コードで停止する

wait $f2_pid
__code2=$?
res2="$(cat "$__future_out2")"
rm -f "$__future_out2"
```

---

## 関数

### 関数定義

```nacre
fn greet(name: String, prefix: String = "Hello"): String {
  "${prefix}, ${name}!"
}

greet("Nacre")           ## "Hello, Nacre!"
greet("Nacre", "Hi")     ## "Hi, Nacre!"
```

**トランスパイル結果:**
Bash の関数内で引数の数をチェックし、デフォルト値を代入するコードに展開される。

```bash
greet() {
  local name="$1"
  local prefix="${2:-Hello}"
  echo "${prefix}, ${name}!"
}
```

### 可変長引数（Rest Parameters）

引数の最後に `...` をつけることで、可変長引数を定義できる。受け取った引数は配列（`[T]`）として扱われる（シェルスクリプトの `$@` に相当）。

```nacre
fn run(cmd: String, args: ...String): Unit \/ CmdError {
  ## args は [String] 型
  for arg in args {
    try $sh'echo "Arg: ${arg}"'
  }
}

run("git", "commit", "-m", "fix")
```

**トランスパイル結果:**

```bash
run() {
  local cmd="$1"
  shift
  local args=("$@")
  for arg in "${args[@]}"; do
    echo "Arg: ${arg}"
  done
}

run "git" "commit" "-m" "fix"
```

### 戻り値

関数の最後の式が戻り値（暗黙の `echo`）。明示的に関数を抜けるための `return` も利用できる。

```nacre
fn add(a: Int, b: Int): Int {
  a + b
}
```

**トランスパイル結果:**

```bash
add() {
  local a="$1"
  local b="$2"
  echo $((a + b))
}
```

### 関数呼び出し

```nacre
const message = greet("Nacre")
const sum = add(1, 2)
```

**トランスパイル結果:**

```bash
message="$(greet "Nacre")"
sum="$(add 1 2)"
```

### メソッド呼び出し (Uniform Function Call Syntax)

Nacre では `obj.method(args)` という形式の呼び出しは、暗黙的に `method(obj, args)` という関数呼び出しとして解釈される（UFCS: Uniform Function Call Syntax）。これには、組み込み型に対するメソッド（`.len()` など）や、`trait` によって定義されたメソッドも含まれる。

これにより、既存の型に対して後付けでメソッドのようなインターフェースを提供することができる。

```nacre
const list = [1, 2, 3]
const size = list.len()    ## len(list) と等価
```

### コマンドを内部で実行する関数

コマンドを内部で実行する関数は `T \/ CmdError` を返す。

```nacre
fn checkFile(path: Path): Unit \/ CmdError {
  $sh"test -f path"
}
```

**トランスパイル結果:**

```bash
checkFile() {
  local path="$1"
  test -f "$path"
}
```

### ラムダ式（無名関数）

`(arg) => expr` の形式で無名関数を定義できる。
ラムダ式は外部スコープのスカラー値を作成時に値として捕捉できる。
配列、Map、レコード、タプルは複数の Bash 変数へ展開されるため、現時点では捕捉できない。
引数型は、代入先の関数型注釈または呼び出し先の関数型パラメーターから推論される。

```nacre
const nums = [1, 2, 3]
const doubled = nums.map(x => x * 2)
```

**トランスパイル結果:**

グローバルな関数として一意な名前で展開される。

```bash
__lambda_1() {
  local x="$1"
  echo $((x * 2))
}
# map の内部で __lambda_1 が呼び出される
```

### ジェネリクス

関数定義や型定義時に `[T]` を使ってジェネリクスを指定できる。

```nacre
fn identity[T](value: T): T {
  value
}

type Wrapper[T] = { value: T }
```

#### 型パラメータの変位（Variance）

ジェネリックな型に対して、`+` と `-` を用いて変位（部分型関係の伝播）を指定できる。

- `+T` : **共変 (Covariant)**（`T` のサブタイプがそのまま許容される。出力・戻り値向き）
- `-T` : **反変 (Contravariant)**（`T` のスーパータイプが許容される。入力・引数向き）
- `T` (修飾なし) : **非変 (Invariant)**（型が完全に一致する必要がある）

```nacre
type Producer[+T] = Unit => T
type Consumer[-T] = T => Unit

## A が B のサブタイプである場合:
## Producer[A] は Producer[B] のサブタイプになる（共変）
## Consumer[B] は Consumer[A] のサブタイプになる（反変）
```

#### 部分型制約（Subtyping bounds）

`[T <: U]`（上限境界）および `[T >: U]`（下限境界）を指定できる。

```nacre
## T は Animal か、そのサブタイプでなければならない
fn makeSound[T <: Animal](animal: T): Unit {
  ## ...
}
```

#### 型クラス指定（Trait constraints）

型パラメータが特定の型クラス（`trait`）を実装していることを要求できる。構文は `[T: TraitName]` を用いる。

```nacre
trait Show[T] {
  fn show(value: T): String
}

## T は Show トレイトを実装していなければならない
fn printValue[T: Show](value: T): Unit {
  log(value.show())
}

## 複数のトレイトを要求する場合は `+` で繋ぐ
fn compareAndPrint[T: Show + Eq](a: T, b: T): Unit {
  ## ...
}
```
---

## 制御構文

### if 式

`if` は式であり、値を返す。

```nacre
const status = if count > 0 {
  "positive"
} else {
  "zero or negative"
}
```

**トランスパイル結果:**

```bash
if [ "$count" -gt 0 ]; then
  status="positive"
else
  status="zero or negative"
fi
```

### パターンマッチ

`match` は式であり、値を返すことができる（`if` と同様）。ネストしたパターンや、`if` によるガード条件もサポートする。

```nacre
const statusMsg = match code {
  200 => "OK",
  404 => "Not Found",
  500 => "Internal Server Error",
  _   => "Unknown",
}
```

```nacre
match signal {
  "HUP"  => try $sh"reloadConfig",
  "TERM" => try $sh"shutdown",
  "INT"  => try $sh"cleanup",
  _      => try $sh"echo \"Unknown signal: ${signal}\"",
}

match response {
  Ok({ status, body }) if status == 200 => try $sh"echo body",
  Ok({ status }) => try $sh"echo \"Error status: ${status}\"",
  Err(e) => try $sh"echo \"Failed: ${e.stderr}\"",
}
```

#### 網羅性のチェック (Exhaustiveness)
Nacre のコンパイラは `match` 式がすべての可能性を網羅しているかをチェックします。列挙型のすべてのケースを記述するか、ワイルドカード `_` を使用してデフォルトケースを記述する必要があります。

`Bool`、`Option`、`Result`、ユーザー定義の列挙型では、ガードなしの
全ケースを列挙すれば `_` を省略できます。ガード付きアームは条件が偽に
なる可能性があるため、網羅性には数えられません。`Int` や `String` など
候補が有限でない型では `_` が必要です。

#### タプルと組み合わせたマッチング
複数の値を同時にマッチングさせることができる。

```nacre
match (statusCode, method) {
  (200, "GET")  => log("Success"),
  (404, _)      => log("Not Found"),
  (_, "DELETE") => log("Delete operation"),
  _             => log("Other"),
}
```

**トランスパイル結果:**

```bash
case "$signal" in
  HUP)
    reloadConfig
    ;;
  TERM)
    shutdown
    ;;
  INT)
    cleanup
    ;;
  *)
    echo "Unknown signal: ${signal}"
    ;;
esac
```

### for ループ

`for` ループの対象は配列（`[T]`）のみに限定される。文字列をループしたい場合は `.split()` で明示的に配列に変換する必要がある。

```nacre
## $sh'...' の戻り値は String なので、.split() で [String] に分割する
for file in try $sh"ls *.txt".split("\n") {
  try $sh"echo \"Processing: ${file}\""
}
```

**トランスパイル結果:**

```bash
IFS=$'\n' read -r -d '' -a __arr < <(ls *.txt && printf '\0')
for file in "${__arr[@]}"; do
  echo "Processing: ${file}"
done
```

#### 並列ループ (`for par`)

`par` キーワードを付与することで、ループの各イテレーションをバックグラウンドで実行し、すべての完了を待機する並列処理を記述できる。

```nacre
for par url in urls {
  try $sh"curl -O ${url}"
}
## すべての curl が完了するまでここで待機する
```

**トランスパイル結果:**
各コマンドに `&` を付与して実行し、最後に `wait` する構造に展開される。

```bash
for url in "${urls[@]}"; do
  curl -O "${url}" &
done
wait
```

### while ループ

```nacre
let i = 0
while i < 10 {
  try $sh"echo \"${i}\""
  i = i + 1
}
```

**トランスパイル結果:**

```bash
i=0
while [ "$i" -lt 10 ]; do
  echo "${i}"
  i=$((i + 1))
done
```

ループ内では `break`（ループを抜ける）と `continue`（次のイテレーションへスキップ）が利用可能。

---

## エラーハンドリング

コマンド実行は常に `T \/ CmdError` を返すため、エラーハンドリングは言語に組み込まれている。

### `CmdError` 型

コマンド実行のエラーを表す組み込み型。終了コードと標準エラー出力の両方を保持する。

```nacre
## 組み込み型の定義（ユーザーが定義する必要はない）
## type CmdError = {
##   code: ExitCode,
##   stderr: String,
## }
```

### `?` 演算子、`!` 演算子、`??` 演算子

| 演算子 | 型変換 | 意味 |
|---|---|---|
| `?` | `T \/ E` → `T?` | Option に変換。エラーを `None` に丸める |
| `!` | `T \/ E` → `T` | 値を取り出す。失敗時は早期リターン |
| `??` | `T \/ E` → `T` | 失敗時にデフォルト値で置き換える |
| `??` | `T?` → `T` | `None` 時にデフォルト値で置き換える |

```nacre
fn deploy(): Unit \/ CmdError {
  ## ! — 値を取り出す（失敗時は早期リターン）
  const branch = $sh"git rev-parse --abbrev-ref HEAD"!

  ## ! — 出力不要、成功を要求
  $sh"git pull"!
  $sh"npm install"!
  $sh"npm run build"!
  $sh"echo \"Deployed ${branch}\""!
}
```

**トランスパイル結果:**

```bash
deploy() {
  local branch
  branch="$(git rev-parse --abbrev-ref HEAD)" || return $?
  git pull || return $?
  npm install || return $?
  npm run build || return $?
  echo "Deployed ${branch}"
}
```

### `??` 演算子（デフォルト値）

`??` は `T \/ E` と `T?` の両方に使える。失敗または `None` のときにデフォルト値で置き換える。

```nacre
## T \/ E に直接使う（エラー時にデフォルト値）
const name: String = $sh"hostname" ?? "unknown"

## T? に使う（None 時にデフォルト値）
const user: String? = findUser(42)
const display: String = user ?? "anonymous"

## ? と組み合わせても同じ
const name2: String = $sh"hostname"? ?? "unknown"
```

**トランスパイル結果:**

```bash
name="$(hostname)" || name="unknown"
display="${user:-anonymous}"
name2="$(hostname)" || name2="unknown"
```

`??` は「失敗しても大丈夫、代わりの値を使う」。

```nacre
## 環境変数（String?）にも使える
const port: String = env.PORT ?? "8080"

## コマンド結果（String \/ CmdError）にも使える
const branch: String = $sh"git rev-parse --abbrev-ref HEAD" ?? "main"

## パイプライン（String \/ CmdError）にも使える
const count: String = ($sh"cat \"data.txt\"" |> $sh"wc -l") ?? "0"
```

**トランスパイル結果:**

```bash
port="${PORT:-8080}"
branch="$(git rev-parse --abbrev-ref HEAD)" || branch="main"
count="$(cat "data.txt" | wc -l)" || count="0"
```

### `match` によるハンドリング

`Err` の中で `code` と `stderr` にアクセスできる。

```nacre
const result = $sh'curl -s "https://example.com"'

match result {
  Ok(body) => {
    try $sh"echo \"Success: ${body}\""
  },
  Err(e) => {
    try $sh"echo \"Failed (${e.code}): ${e.stderr}\""
  },
}
```

**トランスパイル結果:**

```bash
__stderr_tmp=$(mktemp)
if result="$(curl -s "https://example.com" 2>"$__stderr_tmp")"; then
  echo "Success: $result"
else
  __code=$?
  __stderr="$(cat "$__stderr_tmp")"
  echo "Failed (${__code}): ${__stderr}"
fi
rm -f "$__stderr_tmp"
```

### `code` のみ使用する場合の最適化

`stderr` へのアクセスがない場合、トランスパイラは一時ファイルを生成しない。

```nacre
match $sh"curl -s url" {
  Ok(body) => try $sh"echo body",
  Err(e) => try $sh"echo \"Failed with code: ${e.code}\"",
}
```

**トランスパイル結果（最適化版）:**

```bash
if __stdout="$(curl -s url)"; then
  echo "$__stdout"
else
  __code=$?
  echo "Failed with code: ${__code}"
fi
```

---

## 配列

型は `[T]` と記述する。

```nacre
const fruits: [String] = ["apple", "banana", "cherry"]
const first = fruits[0]
const len = fruits.len()

for fruit in fruits {
  try $sh"echo fruit"
}
```

**トランスパイル結果:**

```bash
fruits=("apple" "banana" "cherry")
first="${fruits[0]}"
len="${#fruits[@]}"

for fruit in "${fruits[@]}"; do
  echo "$fruit"
done
```

---

## 連想配列（Map / Dictionary）

型は `Map[K, V]` と記述し、キーと値のペアを扱う（Bash 4.0 以上の連想配列に対応）。

```nacre
const envs: Map[String, String] = {
  "PORT": "8080",
  "HOST": "localhost"
}
const port = envs["PORT"]
```

### コレクションの共通メソッド

`[T]` (Array) および `Map[K, V]` は、以下の主要なメソッドをサポートする（UFCS により関数としても呼び出し可能）。

| 型 | メソッド | 説明 |
|---|---|---|
| 共通 | `.len()` | 要素数を返す |
| 共通 | `.isEmpty()` | 空かどうかを返す |
| Array | `.first()` | 最初の要素を返す |
| Array | `.last()` | 最後の要素を返す |
| Array | `.reverse()` | 要素を逆順にした配列を返す |
| Array | `.sort()` | 要素を辞書順に並べた配列を返す |
| Array | `.unique()` | 重複を取り除いた配列を返す |
| Array | `.map(f)` | 各要素に1引数の関数またはラムダを適用した配列を返す |
| Array | `.push(x)` | 末尾に要素を追加する |
| Array | `.pop()` | 末尾の要素を削除する |
| Array | `.contains(x)` | 要素が含まれるかを確認する |
| Array | `.indexOf(x)` | 最初に一致した要素の位置、なければ `-1` を返す |
| Array | `.slice(start, end)` | 指定範囲の部分配列を返す |
| Array | `.take(count)` | 先頭から指定数の要素を返す |
| Array | `.drop(count)` | 先頭から指定数の要素を除いた配列を返す |
| Array | `.join(sep)` | 文字列として連結する |
| Map | `.keys()` | キーの一覧を配列で返す |
| Map | `.values()` | 値の一覧を配列で返す |
| Map | `.has(key)` | キーの存在を確認する |
| Map | `.set(key, value)` | 可変 Map に要素を追加または更新する |
| Map | `.remove(key)` | 可変 Map から要素を削除する |

**トランスパイル結果:**

```bash
declare -A envs=(
  ["PORT"]="8080"
  ["HOST"]="localhost"
)
port="${envs["PORT"]}"
```

---

## タプル

型は `(A, B, ...)` と記述する。

```nacre
const pair: (String, Int) = ("hello", 42)
const triple = ("a", 1, true)    ## (String, Int, Bool) と推論

const msg = pair._1              ## "hello"
const num = pair._2              ## 42
```

**トランスパイル結果（インデックスベースの変数に展開）:**

```bash
pair_1="hello"
pair_2=42
triple_1="a"
triple_2=1
triple_3=true

msg="$pair_1"
num="$pair_2"
```

---

## 型定義

### レコード型

レコードは `{ ... }` で定義する。

```nacre
type Config = {
  host: String,
  port: Int,
  debug: Bool,
}

const config: Config = {
  host: "localhost",
  port: 8080,
  debug: false,
}

try $sh"echo config.host"
```

**トランスパイル結果（変数プレフィックスに展開）:**

```bash
config_host="localhost"
config_port=8080
config_debug=false

echo "${config_host}"
```

### 列挙型（Sum Types / 代数データ型）

単純な列挙型だけでなく、引数を持つバリアント（代数データ型: ADT）も定義できる。

```nacre
## 単純な列挙型
type LogLevel = Info | Warn | Error

## 引数を持つ代数データ型
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Square(Float)

const c = Circle(10.5)
const r = Rect(10.0, 20.0)

match shape {
  Circle(r)    => log("Circle with radius: ${r}"),
  Rect(w, h)   => log("Rectangle: ${w}x${h}"),
  Square(s)    => log("Square: ${s}"),
}
```

**現在のトランスパイル結果:**
ADT は関数の引数や戻り値として安全に渡せるよう、タグとスカラー
フィールドを unit separator で連結した単一の quoted value に展開されます。

```bash
# Circle(10.5) の概念的な表現
shape=$'Circle\03710.5'

# match の展開
shape_tag="${shape%%$'\037'*}"
shape_1="${shape#*$'\037'}"
if [ "$shape_tag" = "Circle" ]; then
  r="$shape_1"
  echo "Circle with radius: ${r}"
elif [ "$shape_tag" = "Rect" ]; then
  # ...
fi
```

現在、バリアントのフィールドにはスカラー表現を持つ型を使用できます。
配列、Map、レコード、タプルを直接格納するバリアントは未対応です。

### ユニオン型とインターセクション型

構造的なユニオン型（直和型）とインターセクション型（交差型）をサポートする。
これらはコンパイル時の静的型チェッカーでのみ機能し、トランスパイル後のランタイム（シェルスクリプト）では特別な表現を持たない。

#### ユニオン型（`A | B`）

いずれかの型に当てはまることを示す。

```nacre
type Id = String | Int

fn process_id(id: Id): Unit {
  match id {
    s: String => log("String ID: ${s}"),
    n: Int    => log("Int ID: ${n}"),
  }
}
```

#### インターセクション型（`A & B`）

複数の型の要件をすべて満たすことを示す。主にレコード型の合成に利用される。

```nacre
type HasName = { name: String }
type HasAge  = { age: Int }

type Person = HasName & HasAge
## { name: String, age: Int } と等価

const p: Person = { name: "Alice", age: 20 }
```

### ブランド型（`newtype`）

`newtype` で既存の型に名前を付け、型レベルで区別する。
構造的には基底型と同じだが、型チェッカーが異なる型として扱う（公称型）。
ランタイムではゼロコスト（基底型と同じ表現にトランスパイルされる）。

```nacre
newtype UserId = Int
newtype Email = String
newtype Url = String
newtype Celsius = Float
newtype Fahrenheit = Float
```

ブランド型の値は型名を関数として呼び出して作成する。

```nacre
const id = UserId(42)
const email = Email("user@example.com")
const temp = Celsius(36.5)
```

**トランスパイル結果:**

```bash
# ブランド型はランタイムでは消える（ゼロコスト）
id=42
email="user@example.com"
temp="36.5"
```

型チェッカーが `UserId` と `Int` の混同を防ぐ。

```nacre
newtype UserId = Int
newtype GroupId = Int

fn findUser(id: UserId): String? { /* ... */ }

const uid = UserId(1)
const gid = GroupId(2)

findUser(uid)      ## OK
findUser(gid)      ## コンパイルエラー: GroupId は UserId ではない
findUser(1)        ## コンパイルエラー: Int は UserId ではない
```

基底型への変換は `.value` でアクセスする。

```nacre
const uid = UserId(42)
const raw: Int = uid.value       ## 42
const next = UserId(uid.value + 1)
```

**トランスパイル結果:**

```bash
uid=42
raw="$uid"          # .value はそのまま変数参照に
next=$((uid + 1))
```

#### `type` (エイリアス) と `newtype` (ブランド) の違い

| 特徴 | `type` (Alias) | `newtype` (Brand) |
|---|---|---|
| 判定基準 | **構造的一致**（Structural） | **名前的一致**（Nominal） |
| 代入可能性 | 同じ構造なら相互に代入可能 | 明示的な変換なしでは代入不可 |
| ランタイムコスト | ゼロ | ゼロ |
| 主な用途 | 複雑な型への名前付け、簡略化 | ID や単位の区別、バリデーション済みの保証 |

```nacre
type Point = { x: Int, y: Int }
newtype Latitude = Float

## Point 型は同じレコード構造なら代入できるが、
## Latitude 型は単なる Float とは区別される。
```

ブランド型にメソッドを追加することもできる。

```nacre
newtype Celsius = Float
newtype Fahrenheit = Float

fn toFahrenheit(c: Celsius): Fahrenheit {
  Fahrenheit(c.value * 1.8 + 32.0)
}

const bodyTemp = Celsius(36.5)
const f = toFahrenheit(bodyTemp)   ## Fahrenheit(97.7)
```

**トランスパイル結果:**

```bash
toFahrenheit() {
  local c="$1"
  echo "$(echo "$c * 1.8 + 32.0" | bc)"
}

bodyTemp="36.5"
f="$(toFahrenheit "$bodyTemp")"
```

#### コンストラクタのオーバーライド（スマートコンストラクタ）

デフォルトでは、`newtype Celsius = Float` と定義すると `Celsius(value: Float): Celsius` という暗黙のコンストラクタが生成される。
このコンストラクタは、**同一ファイル内（モジュール内）に限り**、`fn!` キーワードを用いて明示的に再定義（オーバーライド）することができる。`fn!` を使うことで、意図しない名前衝突や意図しない上書きを防ぐ。

これを利用して、バリデーションを伴うスマートコンストラクタ（戻り値を `Celsius?` や `Celsius \/ CmdError` などにする）を定義できる。

```nacre
newtype Celsius = Float

## デフォルトのコンストラクタを明示的にオーバーライド
fn! Celsius(value: Float): Celsius \/ CmdError {
  if value < -273.15 {
    ## エラーを返す
    ## （CmdError のコンストラクタ等を使用してエラー表現）
  } else {
    ## 型キャスト（as）を用いてインスタンスを生成する
    value as Celsius
  }
}
```

オーバーライドは同一ファイル内でのみ許容されるため、外部のモジュールからはこの安全なコンストラクタを経由しなければ `Celsius` 型の値を生成できなくなる（不正な値の混入を型レベルで防げる）。


### 型クラス（`trait`）

型クラス（`trait`）を定義し、型に対して共通のインターフェース（メソッドや演算子）を提供できる。
コンパイル時に静的に解決（モノモルフィゼーション）されるため、実行時のオーバーヘッドはない。

```nacre
trait Functor[F] {
  fn map[A, B](fa: F[A], f: A => B): F[B]
}

trait Applicative[F] {
  fn pure[A](a: A): F[A]
  fn ap[A, B](ff: F[A => B], fa: F[A]): F[B]
}

trait Monad[M] {
  fn flatMap[A, B](ma: M[A], f: A => M[B]): M[B]
}

## 演算子のエイリアス
## `<$>` 演算子は `map`
## `<*>` 演算子は `ap`
## `>>=` 演算子は `flatMap`
## `<|>` 演算子は `orElse`（Alternative的フォールバック）
```

**実装例 (`Option` の場合)**

```nacre
impl Functor[Option] {
  fn map[A, B](fa: A?, f: A => B): B? {
    match fa {
      Some(a) => Some(f(a)),
      None => None,
    }
  }
}

impl Monad[Option] {
  fn flatMap[A, B](ma: A?, f: A => B?): B? {
    match ma {
      Some(a) => f(a),
      None => None,
    }
  }
}
```

**使用例**

```nacre
const user_id = Some(42)

## map メソッドとしての呼び出し
const user_str1 = user_id.map(id => "User: ${id}")

## `<$>` (map) 演算子の使用
const user_str2 = user_id <$> (id => "User: ${id}")

## `>>=` (flatMap) 演算子の使用
const profile = user_id >>= fetch_user_profile

## `<|>` (orElse) 演算子によるフォールバック
const final_id = user_id <|> Some(0)
```

#### 衝突の解決と実装のコヒーレンス（Orphan Rule）

型クラスには、予期せぬ挙動を防ぐための2つの重要なルールがある。

1. **メソッド名の衝突**: 2つの異なるトレイトが同じ名前のメソッドを持っており、ある型が両方を実装している場合、`obj.method()` という呼び出しは曖昧さのためコンパイルエラーとなる。この場合、`TraitName.method(obj)` のようにトレイト名で明示的にスコープを指定して呼び出す必要がある。

2. **オーファンルール（孤児ルール）**: 実装（`impl`）が許可されるのは、その「トレイト自身」または「対象となる型」のどちらかが、現在のモジュール（ファイル）で定義されている場合のみ。これにより、無関係なサードパーティ製ライブラリ同士が、同じ標準型（`String` 等）に対して同じトレイトを競合して実装することを防ぐ。

#### 同一型に対する複数の実装（Newtype パターン）

「数値の加法モノイド」と「数値の乗法モノイド」のように、同じ型に対して複数の異なる型クラス実装が必要な場合は、`newtype` を用いて型を分けることで解決する。

```nacre
newtype Sum = Int
newtype Product = Int

impl Monoid[Sum] {
  fn combine(a: Sum, b: Sum): Sum { (a.value + b.value) as Sum }
  fn empty(): Sum { 0 as Sum }
}

impl Monoid[Product] {
  fn combine(a: Product, b: Product): Product { (a.value * b.value) as Product }
  fn empty(): Product { 1 as Product }
}
```

**バリデーションの例 (`Validation` と Applicative)**

`Result`（`\/`）や `Monad` が最初のエラーで短絡（早期リターン）するのに対し、`Applicative` を用いるとエラーを蓄積（集約）できる。

```nacre
## エラーを配列で蓄積する型（標準ライブラリなどで提供）
type Validation[E, A] = Valid(A) | Invalid([E])

## Validation を返す関数
fn validate_name(name: String): Validation[String, String] { /* ... */ }
fn validate_age(age: Int): Validation[String, Int] { /* ... */ }

## カリー化されたコンストラクタ関数
const make_user = (name: String) => (age: Int) => User({ name, age })

## `<$>` と `<*>` を使って複数のバリデーションを実行。
## 失敗した場合は Invalid(["Invalid name", "Invalid age"]) のように
## 全てのエラーが集約される。
const result = make_user
  <$> validate_name("Alice")
  <*> validate_age(20)
```

**トランスパイル結果:**
トランスパイラは、型クラスのメソッド呼び出しを対象の型に特化した通常の関数呼び出しに静的に展開する。

```bash
# 展開後、純粋なシェルスクリプト関数として出力される
__Option_map_String() { ... }
__Option_flatMap_Profile() { ... }
```

### do 構文 (do-notation)

`flatMap`（`>>=`）の連鎖を命令型言語のように読みやすく書くための糖衣構文として `do` ブロックを提供する。

```nacre
const profile = do {
  user_id <- fetch_user_id()
  user <- fetch_user(user_id)
  pure(user.profile)  ## Monad の pure 関数
}
```

これはコンパイル時に以下の `flatMap` の連鎖に脱糖（desugar）される。

```nacre
fetch_user_id() >>= (user_id =>
  fetch_user(user_id) >>= (user =>
    pure(user.profile)
  )
)
```

`<-` によってモナドから値を取り出して束縛するだけでなく、通常の変数宣言（`let` や `const`）を記述することもできる。

```nacre
const result = do {
  user_id <- fetch_user_id()
  const prefix = "User: "
  user <- fetch_user(user_id)
  pure("${prefix}${user.profile}")
}
```

### Option 型（`T?`）

```nacre
fn findUser(id: Int): String? {
  ## ユーザーが見つからない場合は None
}

const user = findUser(42)
const name = user ?? "unknown"    ## None の場合のデフォルト値
```

### Result 型（`T \/ E`）

コマンド実行は自動的に `T \/ CmdError` 型になる。

```nacre
fn fetch(url: String): String \/ CmdError {
  $sh"curl -s url"    ## そのまま返す（String \/ CmdError）
}

fn fetch_or_default(url: String): String {
  $sh"curl -s url" ?? ""  ## 失敗時は空文字列
}
```

---

## 可視性 (Visibility)

ファイルやモジュール間のアクセス制御は**命名規則**によって行われる。

- 先頭が `_`（アンダースコア）で始まる宣言（`_name`, `_helper()`）: **ファイルローカル**（プライベート扱い、他のファイルから参照不可）
- それ以外の通常の宣言（`name`, `helper()`）: **パブリック**（暗黙的に外部公開される）

---

## モジュールシステム

```nacre
## lib/utils.ncr
fn log(msg: String) {
  const d = try $sh"date '+%Y-%m-%d %H:%M:%S'"
  try $sh'echo "[${d}] ${msg}"'
}
```

```nacre
## main.ncr
use lib.utils

utils.log("Starting application")
```

#### モジュール解決のルール
`use a.b.c` は、カレントディレクトリまたは `NACRE_PATH` を起点に、以下の順序でファイルを探索する。
1. `a/b/c.ncr`
2. `a/b/c/index.ncr`

トランスパイル後は Bash の `source` 命令に変換され、相対パスは実行時のスクリプト位置を基準に解決される。

**トランスパイル結果:**

```bash
# main.sh
source "$(dirname "$0")/lib/utils.sh"

log "Starting application"
```

---

## 特殊構文

### ヒアドキュメント

```nacre
$sh"cat << EOF
  Hello, ${name}
  Today is ${date}
EOF"
```

### シバン

```nacre
#! /usr/bin/env bash
## Nacre ファイルの先頭にシバンを書ける
```

### シェルコードの埋め込み（エスケープハッチ）

```nacre
raw {
  trap 'cleanup' EXIT
  set -euo pipefail
}
```

**トランスパイル結果:**

```bash
trap 'cleanup' EXIT
set -euo pipefail
```

---

## コメント

- `##` : 通常のコメント
- `###` : ドキュメントコメント（ツールチェインで抽出可能）

```nacre
### ユーザー情報を取得する
### 失敗した場合は None を返す
fn findUser(id: Int): String? {
  ## キャッシュを確認
  ## ...
}
```

---

トップレベルに `main` 関数は要求されない。ファイルの上から順にトップレベルの式が評価・実行されるスクリプト言語らしい動作を基本とする。

### コマンドライン引数
スクリプトに渡された引数は、組み込みの `args`（`[String]` 型）または
`std.process` の `process.args()` を通じてアクセスできる。

```nacre
## 引数の分解代入
const [command, target, ...options] = process.args()
const home = process.env("HOME")
const uname = process.exec("uname -s")
const here = process.cwd()
process.chdir("/tmp")

if args.len() < 2 {
  try $sh"echo \"Usage: nacre-script <command> <target>\""
  process.exit(1)
}
```

### シグナルハンドリング
プロセスのシグナル（SIGINT, SIGTERM 等）を、引数なしの関数を渡してハンドリングできる。

```nacre
fn cleanupInterrupt(): Unit {
  log.info("Interrupted! Cleaning up...")
  cleanup()
  process.exit(130)
}
process.onSignal("INT", cleanupInterrupt)
```

#### `process.onExit(handler: () => Unit): Unit`
スクリプトが（正常終了・エラー終了に関わらず）終了する際に実行されるクリーンアップ処理を登録できる。

```nacre
const tmp = fs.createTempDir()
fn cleanupTmp(): Unit {
  fs.remove(tmp)
}
process.onExit(cleanupTmp)
```

**トランスパイル結果:**
Bash の `trap ... EXIT` 命令に展開される。

**トランスパイル結果:**
Bash の `trap` 命令と、シグナルハンドラ関数に展開される。

---

## プログラム例

### 完全なスクリプト例

```nacre
#! /usr/bin/env bash

use std.fs
use std.log

const targetDir = env.TARGET_DIR ?? "./build"

if !fs.exists(targetDir) {
  try fs.mkdirP(targetDir)
}

const files = try $sh"find \"src\" -name \"*.txt\""

for file in files.split("\n") {
  log.info("Processing: ${file}")
  const content = try $sh"cat file"
  const processed = try (content |> $sh"tr 'a-z' 'A-Z'" |> $sh"sort")
  $sh'echo "${processed}"' >> write("${targetDir}/${fs.basename(file)}")
}

log.info("Done. Processed ${files.len()} files.")
```

**トランスパイル結果:**

```bash
#!/usr/bin/env bash
source "$(dirname "$0")/std/fs.sh"
source "$(dirname "$0")/std/log.sh"

targetDir="${TARGET_DIR:-./build}"

if [ ! -e "$targetDir" ]; then
  mkdir -p "$targetDir"
fi

IFS=$'\n' read -r -d '' -a __files < <(find "src" -name "*.txt" && printf '\0')
for file in "${__files[@]}"; do
  log_info "Processing: ${file}"
  content="$(cat "$file")" || exit $?
  processed="$(echo "$content" | tr 'a-z' 'A-Z' | sort)"
  echo "$processed" > "${targetDir}/$(basename "$file")"
done

log_info "Done. Processed files."
```

---

## ファイル拡張子

`.ncr`

---

## 型構文まとめ

| 記法 | 意味 | 例 |
|---|---|---|
| `T` | プリミティブ型 | `String`, `Int`, `Float`, `Bool` |
| `[T]` | 配列 | `[String]`, `[Int]` |
| `Map[K, V]` | 連想配列 | `Map[String, Int]` |
| `T?` | Option（省略可能） | `String?`, `Int?` |
| `T \/ E` | Result（成功 or エラー） | `String \/ CmdError` |
| `A \| B` | ユニオン型 | `String \| Int` |
| `A & B` | インターセクション型 | `HasName & HasAge` |
| `{ ... }` | レコード | `{ host: String, port: Int }` |
| `(A, B)` | タプル | `(String, Int)` |
| `newtype T = U` | ブランド型 | `newtype UserId = Int` |
| `A => B` | 関数型 | `String => Int`, `(Int, Int) => Bool` |

---

## Bash vs Nacre 逆引きチートシート

| 実現したいこと | Bash | Nacre |
|---|---|---|
| ファイルの存在確認 | `if [ -f "$f" ];` | `if fs.exists(f) {` |
| 文字列の比較 | `if [ "$a" = "$b" ];` | `if a == b {` |
| 終了コードの取得 | `cmd; code=$?` | `match $sh"cmd" { Err(e) => e.code, ... }` |
| デフォルト値付き変数 | `${VAR:-default}` | `env.VAR ?? "default"` |
| 配列への追加 | `arr+=("val")` | `arr.push("val")` |
| コマンド出力のループ | `for f in $(ls); do` | `for f in (try $sh"ls").split("\n") {` |
| 早期リターン（失敗時） | `cmd || exit $?` | `try $sh"cmd"` |

---

## テスト (`std.test`)

信頼性の高いシェルスクリプトを構築するため、`std.test` は実行時
アサーションを提供する。

```nacre
use std.test

const s = "a,b,c"
const parts = s.split(",")
test.assert(parts.len() == 3)
test.assert(parts[0] == "a", "first element should be a")
```

`test.assert(condition, message = "assertion failed")` は条件が偽のとき
メッセージを標準エラーへ出力し、終了コード 1 で停止する。
- テストは通常実行時には無視され、テスト用フラグを立てて実行した際のみ抽出・実行される。

---

## 並列実行とジョブ管理

Bash の `&` と `wait` を抽象化し、型安全なバックグラウンド実行をサポートする。
`spawn $sh"..."` は `async $sh"..."` と同じく `Future[String]` を返す。

```nacre
## バックグラウンドでジョブを開始
const job1 = spawn $sh"sleep 5; printf 'Job 1 done'"
const job2 = spawn $sh"sleep 2; printf 'Job 2 done'"

log.info("Jobs are running in background...")

## ジョブの終了を待機し、標準出力を回収
const res1 = job1.wait()
const res2 = job2.wait()
```

- `spawn` は `Future[String]` 型を返す。
- `.wait()` メソッドにより、プロセスの終了を同期し、標準出力を取得できる。
- バックグラウンドコマンドが失敗した場合は、その終了コードでスクリプトを停止する。

---

## 高度なコマンドライン引数解析 (`std.cli`)

`args` を直接操作する代わりに、長い形式の引数やフラグを `Map[String, String]`
に解析できる。

```nacre
use std.cli

const options = cli.parse()

if options.has("force") {
  log.info("Force mode enabled")
}

const output = options["output"]
```

`cli.parse()` は `--name value`、`--name=value`、`--flag` をサポートする。
フラグ値は文字列 `"true"` として格納される。

---

## 外部ライブラリの型定義 (`.d.ncr`)

既存の Bash ライブラリ（`.sh` ファイル）を Nacre から安全に呼び出すための定義ファイル。実装を持たず、型シグネチャのみを記述する。

```nacre
## libexternal.d.ncr
export fn externalFunction(arg: String): Int \/ CmdError
```

利用側：
```nacre
use libexternal

const result = try libexternal.externalFunction("test")
```

---

## 標準ライブラリ構成案 (Planned)

命名規則は **`camelCase`** を採用し、モダンな言語としての統一感を図る。

- `std.fs` : ファイル・ディレクトリ操作（`exists`, `isFile`, `isDir`, `size`, `mkdirP`, `remove`, `copy`, `move`, `touch`, `createTempDir`, `readText`, `readLines`, `list`, `basename`, `dirname`, `stem`, `extname`, `writeText`, `appendText`, `writeLines`, `appendLines`）
- `std.path` : パス文字列操作（`join`, `isAbsolute`, `basename`, `dirname`, `stem`, `extname`）
- `std.process` : プロセス管理（`exit`, `args`, `env`, `hasCommand`, `exec`, `cwd`, `chdir`, `onSignal`, `onExit`）
- `std.cli` : コマンドライン引数解析（`parse`）
- `std.test` : ユニットテストサポート（`assert`）
- `std.io` : 対話型入出力（`prompt`, `confirm`, `promptPassword`）
- `std.log` : ロギング（`info`, `warn`, `error`, `debug`）
- `std.str` : 文字列操作（`split`, `join`, `len`, `isEmpty`, `slice`, `trim`, `trimStart`, `trimEnd`, `contains`, `indexOf`, `startsWith`, `endsWith`, `toUpper`, `toLower`, `repeat`, `replace`）
- `std.json` : JSON パース・生成（`parse`, `stringify`）

---

## 決定事項

- **命名規則**: 標準ライブラリおよびユーザコードにおいて、関数・変数は `camelCase`、型・トレイトは `PascalCase` を標準とする。
- **インライン展開**: 標準ライブラリは、トランスパイル時に必要なコードのみが生成スクリプト内に**インライン展開（埋め込み）**される。これにより、生成された `.sh` ファイル単体でのポータビリティを確保する。
- **外部依存のチェック**: 複雑な処理（JSON 等）において `jq` 等の外部ツールを必要とする場合は、`require` 構文によって実行時に自動チェックされる。
