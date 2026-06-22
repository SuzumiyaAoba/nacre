use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use nacre::{compile_file, compile_source, compile_source_with_policy, ExecutionPolicy};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nacre-surface-{name}-{unique}"))
}

fn env_policy(names: &[&str]) -> ExecutionPolicy {
    let read = names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    ExecutionPolicy::from_toml(
        &format!("[environment]\nread = [{read}]\n"),
        std::path::Path::new("."),
    )
    .unwrap()
}

#[test]
fn compiles_comprehensive_structural_surface() {
    compile_source_with_policy(
        r#"
const answer = 42
const hex: Int = 0xFF
const bits = 0b1010
const pi: Float = 3.14
const yes = true
const no = false
const unit: Unit = ()
const bin: Path = "/usr/bin"
const names: [String] = ["alice", "bob"]
fn greet(name: String, prefix: String = "Hello"): String {
return "${prefix}, ${name}"
}
const message = greet("Nacre")
const namedMessage = greet(prefix = "Hi", name = "Nacre")
type Unary = String => String
fn exclaim(value: String): String {
return "${value}!"
}
fn applyString(f: Unary, value: String): String {
return f(value)
}
const applied = applyString(exclaim, "Hi")
fn summarize(prefix: String, values: ...String): String {
const firstValue = values[0]
const valueCount = values.len()
return "${prefix}:${firstValue}:${valueCount}"
}
const summary = summarize("rest", "one", "two", "three")
fn identity[T](value: T): T {
return value
}
const genericText = identity("generic")
const genericInt = identity(7)
const methodApplied = genericText.exclaim()
trait Show[T] {
fn show(value: T): String
}
impl Show[Int] {
fn show(value: Int): String {
return "int ${value}"
}
}
impl Show[String] {
fn show(value: String): String {
return "string ${value}"
}
}
fn boundedIdentity[T: Show](value: T): T {
return value
}
const boundedInt = boundedIdentity(7)
const shownInt = Show.show(boundedInt)
const shownText = genericText.show()
type Box[T] = { item: T }
const boxed: Box[Int] = { item: 7 }
const boxedValue = boxed.item
const status = if answer > 0 { "positive" } else { "zero" }
const matched = match status { "positive" => "matched", _ => "fallback" }
let nums = [1, 2, 3]
nums = [4, 5]
const envs: Map[String, String] = { "PORT": "8080", "HOST": "localhost" }
const port = envs["PORT"]
const pair: (String, Int) = ("localhost", 8080)
const hostName = pair._1
const portNumber = pair._2
const (destructuredHost, destructuredPort) = pair
const user: { name: String, age: Int } = { name: "Ada", age: 36 }
const { name, age } = user
type Account = { id: Int, name: String }
const account: Account = { id: 1, name: "core" }
newtype UserId = Int
const uid: UserId = UserId(42)
const rawUid: Int = uid.value
const text = "a'b"
const greeting = "Hello, ${text}"
const rawText = r"keep \n raw"
let joinedText = "join"
joinedText ++= "ed"
const shell = env.SHELL
const home = env.HOME ?? "/tmp"
let count = 10
count -= 2 / 1 % 2
count += 1
count *= 2
count /= 2
count %= 10
let flags = 1
flags <<= 2
flags |= 1
flags &= 5
flags ^= 1
flags >>= 1
let widened: Float = answer
const eq = answer == 42
const ne = answer != 0
const floatCmp = pi > 3
const lt = 1 < 2
const le = 1 <= 2
const gt = 2 > 1
const ge = 2 >= 1
const bools = true == false
const logical = true && !false || bools
if count > 0 {
const branch = "positive"
} else {
const branch = "zero"
}
while count > 0 {
if count == 1 {
break
}
count = count - 1
continue
}
defer {
const cleaned = true
}
for person in names {
const copiedPerson = person
}
for index in 0..3 {
const copiedIndex = index
}
for (left, right) in [("a", 1), ("b", 2)] {
const copiedLeft = left
const copiedRight = right
}
for { nickname } in [{ nickname: "Ada" }] {
const copiedNickname = nickname
}
"#,
        &env_policy(&["SHELL", "HOME"]),
    )
    .unwrap();
}

#[test]
fn compiles_collection_string_and_path_methods() {
    compile_source(
        r#"
fn decorate(value: String): String {
return "[${value}]"
}
let names = ["bob", "alice alpha", "bob"]
const namesLen = names.len()
const namesEmpty = names.isEmpty()
const first = names.first()
const last = names.last()
const reversed = names.reverse()
const sorted = names.sort()
const unique = names.unique()
const mapped = names.map(decorate)
const contains = names.contains("bob")
const index = names.indexOf("alice alpha")
const missingIndex = names.indexOf("carol")
const sliced = names.slice(0, 2)
const taken = names.take(2)
const dropped = names.drop(1)
const joined = names.join("|")
names.push("carol")
names.pop()
let ports = { "http": 80, "https": 443 }
const mapLen = ports.len()
const mapEmpty = ports.isEmpty()
const keys = ports.keys()
const values = ports.values()
const hasHttp = ports.has("http")
ports.set("admin", 9000)
ports.remove("admin")
const text = "  Nacre Nacre  "
const textLen = text.len()
const textEmpty = text.isEmpty()
const textSlice = text.slice(2, 7)
const trimmed = text.trim()
const left = text.trimStart()
const right = text.trimEnd()
const upper = text.toUpper()
const lower = text.toLower()
const repeated = "na".repeat(3)
const split = "a,b,c".split(",")
const replaced = text.replace("Nacre", "Safe")
const textContains = text.contains("Nacre")
const textIndex = text.indexOf("Nacre")
const starts = trimmed.startsWith("Nacre")
const ends = trimmed.endsWith("Nacre")
const path: Path = "/tmp/archive.tar.gz"
const absolute = path.isAbsolute()
const base = path.basename()
const dir = path.dirname()
const stem = path.stem()
const ext = path.extname()
"#,
    )
    .unwrap();
}

#[test]
fn compiles_added_standard_library_helpers() {
    let dir = temp_dir("std-helpers");
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("main.ncr");
    fs::write(
        &input,
        r#"
use std.str
use std.path
const lines = str.lines("""a
b""")
const words = str.words("  a b  ")
const normalized = path.normalizeSlashes("/tmp//nacre")
"#,
    )
    .unwrap();
    compile_file(&input).unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn compiles_union_and_intersection_annotations() {
    compile_source(
        r#"
const stringOrPath: String | Path = "input.txt"
const bothTextPath: String & Path = "input.txt"
fn stringify(value: String | Path): String {
return value as String
}
const text = stringify(stringOrPath)
"#,
    )
    .unwrap();
}

#[test]
fn compiles_option_result_lambda_match_and_do_surface() {
    compile_source(
        r#"
fn length(value: String): Int {
return value.len()
}
fn positive(value: Int): Int? {
if value > 0 {
return Some(value)
}
return None
}
fn parse(value: String): Int \/ String {
if value.isEmpty() {
return Err("empty")
}
return Ok(value.len())
}
fn add(left: Int, right: Int): Int {
return left + right
}
fn increment(value: Int): Int {
return value + 3
}
const present: String? = Some("nacre")
const missing: String? = None
const mapped = present.map(length)
const flatMapped = mapped.flatMap(positive)
const fallback = missing.orElse(Some("fallback"))
const alternative = missing <|> Some("alternative")
const wrappedLength: Option[String => Int] = Some(length)
const appliedOption = wrappedLength.ap(present)
const ok: Int \/ String = Ok(7)
const err: Int \/ String = Err("bad")
const resultMapped = ok.map(value => value + 1)
const resultFlat = ok.flatMap(value => Ok(value + 2))
const wrappedAdd: Result[Int => Int, String] = Ok(increment)
const appliedResult = wrappedAdd.ap(ok)
const optionValue = mapped ?? 0
const resultValue = ok ?? 0
const optionDo: Int? = do {
text <- present
size <- Some(text.len())
pure(size)
}
const resultDo: Int \/ String = do {
value <- ok
pure(value + 1)
}
type Payload = Text(String) | Pair(Int, Int) | Empty
fn describe(value: Payload): String {
return match value {
Text(text) if !text.isEmpty() => text,
Pair(left, right) => "${left}:${right}",
Empty => "empty",
_ => "blank"
}
}
const describedText = describe(Text("value"))
const describedPair = describe(Pair(1, 2))
const describedEmpty = describe(Empty)
const tuple = ("host", 8080)
const tupleMatch = match tuple { (host, port) => "${host}:${port}", _ => "none" }
const record = { name: "Ada", age: 36 }
const recordMatch = match record { { name, age } => "${name}:${age}", _ => "none" }
"#,
    )
    .unwrap();
}

#[test]
fn compiles_builtin_and_policy_capability_surface() {
    let dir = temp_dir("capabilities");
    let root = dir.join("root");
    fs::create_dir_all(&root).unwrap();
    let command = dir.join("command");
    fs::write(
        &command,
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&command).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&command, permissions).unwrap();
    }
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        "[environment]\nread = [\"HOME\"]\n\n[process]\nargs = true\n\n[filesystem]\nread = [\"root\"]\nwrite = [\"root\"]\n\n[command_groups.read.commands.file]\nprogram = \"command\"\nargs = 1\nread_args = [0]\n\n[command_groups.output.commands.print]\nprogram = \"command\"\nargs = 1\n",
    )
    .unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let input = root.join("input.txt");
    let output = root.join("output.txt");
    let source = format!(
        r#"
const exists = pathExists({input:?})
const isFile = fs.isFile({input:?})
const isDir = fs.isDir({root:?})
const size = fs.size({input:?})
const lines = fs.readLines({input:?})
const entries = fs.list({root:?})
fs.writeLines({output:?}, ["one", "two"])
fs.appendLines({output:?}, ["three"])
const envValue = process.env("HOME")
const parsedArgs = cli.parse()
const parsedJson = json.parse("{{\"name\":\"Nacre\"}}")
const encodedJson = json.stringify({{ "name": "Nacre" }})
const commandOutput = run.read.file({input:?})
run.output.print(commandOutput)
"#,
        input = input,
        root = root,
        output = output,
    );
    compile_source_with_policy(&source, &policy).unwrap();
    fs::remove_dir_all(dir).unwrap();
}
