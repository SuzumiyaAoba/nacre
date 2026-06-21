use std::fs;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_source(source: &str, trailer: &str, args: &[&str]) -> Output {
    run_source_with_policy(source, &nacre::ExecutionPolicy::deny_all(), trailer, args)
}

fn run_source_with_policy(
    source: &str,
    policy: &nacre::ExecutionPolicy,
    trailer: &str,
    args: &[&str],
) -> Output {
    let mut bash = nacre::compile_source_with_policy(source, policy).unwrap();
    bash.push('\n');
    bash.push_str(trailer);
    Command::new("bash")
        .arg("-c")
        .arg(bash)
        .arg("nacre-test")
        .args(args)
        .output()
        .unwrap()
}

fn run_file(path: &std::path::Path, trailer: &str) -> Output {
    let mut bash = nacre::compile_file(path).unwrap();
    bash.push('\n');
    bash.push_str(trailer);
    Command::new("bash")
        .arg("-c")
        .arg(bash)
        .arg("nacre-test")
        .output()
        .unwrap()
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nacre-{name}-{unique}"))
}

#[test]
fn public_api_accessors_and_parse_errors() {
    let program = nacre::parse("const answer = 42\n").unwrap();
    assert_eq!(program.statements().len(), 1);

    let error = nacre::compile_source("const bad-name = 1").unwrap_err();
    assert_eq!(error.line(), 1);
    assert_eq!(error.column(), 1);
    assert_eq!(error.end_line(), 1);
    assert_eq!(error.source_name(), Some("<source>"));
    assert_eq!(error.source_line(), Some("const bad-name = 1"));
    assert!(error.message().contains("invalid variable name"));
    assert!(error.to_string().contains("^"));

    let syntax = nacre::compile_source("const value = @").unwrap_err();
    assert_eq!(syntax.line(), 1);
    assert!(syntax.column() > 1);
    assert!(syntax.message().contains("invalid syntax"));
}

#[test]
fn public_api_type_checks_structured_programs() {
    let program = nacre::parse(
        r#"
trait Show[T] {
fn show(value: T): String
}
impl Show[Int] {
fn show(value: Int): String {
return "int ${value}"
}
}
type Id[T] = T
newtype UserId = Int
fn choose(value: Int): Int {
if value > 0 {
return value
} else {
return 0
}
}
const names = ["a", "b"]
let count = 2
count = count - 1
for name in names {
const copy = name
}
const uid: UserId = UserId(1)
const rawId: Int = uid.value
const shown = Show.show(choose(count))
let mutable: Id[Int] = rawId
mutable = 2
"#,
    )
    .unwrap();

    nacre::type_check(&program).unwrap();
}

#[test]
fn public_api_reports_representative_type_errors() {
    let cases = [
        ("const x = 1\nconst x = 2", "already defined"),
        ("let x = 1\nx = true", "type mismatch"),
        ("if 1 {\nconst x = 1\n}", "condition must be Bool"),
        (
            "const x = 1\nfor item in x {\nconst y = item\n}",
            "for loop iterable must be Array",
        ),
        (
            "fn greet(value: String): String {\nreturn value\n}\nconst x = greet(1)",
            "argument `value`",
        ),
        (
            "const x = { name: \"Ada\" }\nconst y = x.age",
            "has no field `age`",
        ),
        (
            "const values = [1, 2]\nconst first: String = values[0]",
            "type annotation mismatch",
        ),
        (
            "const pair = (\"host\", 8080)\nconst port: String = pair._2",
            "type annotation mismatch",
        ),
        (
            "type Payload = Text(String) | Empty\nconst value = Text(1)",
            "argument 1 for variant `Text`",
        ),
    ];

    for (source, expected) in cases {
        let program = nacre::parse(source).unwrap();
        let error = nacre::type_check(&program).unwrap_err();
        assert!(error.message().contains(expected), "{error}");
    }
}

#[test]
fn generated_bash_runs_core_values_and_control_flow() {
    let output = run_source(
        r#"
const answer = 42
const pi = 3.5
const same = answer == 42
const greater = pi > 3
const joined = "na" ++ "cre"
const trimmed = "  safe  ".trim()
const upper = joined.toUpper()
const parts = "a,b,c".split(",")
const middleParts = parts.slice(1, 3)
const middle = middleParts.join("|")
let count = 2
let looped = ""
while count > 0 {
looped = looped ++ "${count}"
count = count - 1
}
const label = if same && greater { "ok" } else { "bad" }
const matched = match label { "ok" => "matched", _ => "fallback" }
"#,
        "printf '%s|%s|%s|%s|%s|%s\\n' \"$joined\" \"$trimmed\" \"$upper\" \"$middle\" \"$looped\" \"$matched\"",
        &[],
    );

    assert_eq!(stdout(output), "nacre|safe|NACRE|b|c|21|matched\n");
}

#[test]
fn generated_bash_preserves_definite_and_implicit_returns() {
    let output = run_source(
        r#"
fn choose(flag: Bool): String {
if flag {
return "explicit"
}
"implicit"
}

fn classify(flag: Bool): String {
if flag {
return "yes"
} else {
return "no"
}
}

const explicit = choose(true)
const implicit = choose(false)
const yes = classify(true)
const no = classify(false)
"#,
        "printf '%s|%s|%s|%s\\n' \"$explicit\" \"$implicit\" \"$yes\" \"$no\"",
        &[],
    );

    assert_eq!(stdout(output), "explicit|implicit|yes|no\n");
}

#[test]
fn generated_bash_runs_functions_generics_traits_and_newtypes() {
    let output = run_source(
        r#"
fn greet(name: String, prefix: String = "Hello"): String {
return "${prefix}, ${name}"
}
fn identity[T](value: T): T {
return value
}
trait Show[T] {
fn show(value: T): String
}
impl Show[Int] {
fn show(value: Int): String {
return "int:${value}"
}
}
newtype UserId = Int
const greeting = greet("Nacre")
const custom = greet("Nacre", "Hi")
const generic = identity("value")
const shown = Show.show(7)
const userId: UserId = UserId(9)
const rawId: Int = userId.value
"#,
        "printf '%s|%s|%s|%s|%s\\n' \"$greeting\" \"$custom\" \"$generic\" \"$shown\" \"$rawId\"",
        &[],
    );

    assert_eq!(stdout(output), "Hello, Nacre|Hi, Nacre|value|int:7|9\n");
}

#[test]
fn generated_bash_runs_options_results_and_do_expressions() {
    let output = run_source(
        r#"
fn length(value: String): Int {
return value.len()
}
fn requireText(value: String): Int \/ String {
if value.isEmpty() {
return Err("empty")
}
return Ok(value.len())
}
const present: String? = Some("nacre")
const missing: String? = None
const mapped = present.map(length) ?? 0
const fallback = missing ?? "fallback"
const ok: Int \/ String = requireText("safe")
const err: Int \/ String = requireText("")
const okValue = match ok { Ok(value) => value, _ => 0 }
const errValue = match err { Err(message) => message, _ => "none" }
const viaDo: Int? = do {
text <- present
pure(text.len())
}
const doValue = viaDo ?? 0
"#,
        "printf '%s|%s|%s|%s|%s\\n' \"$mapped\" \"$fallback\" \"$okValue\" \"$errValue\" \"$doValue\"",
        &[],
    );

    assert_eq!(stdout(output), "5|fallback|4|empty|5\n");
}

#[test]
fn generated_bash_runs_arrays_maps_and_destructuring() {
    let output = run_source(
        r#"
let names = ["bob", "alice alpha"]
names.push("carol")
const first = names.first()
const last = names.last()
const sortedNames = names.sort()
const sorted = sortedNames.join("|")
const reversedNames = names.reverse()
const reversed = reversedNames.join("|")
const [head, ...tail] = names
let ports = { "http": 80 }
ports.set("https", 443)
const hasHttps = ports.has("https")
const https = ports["https"]
const pair = ("host", 8080)
const (host, port) = pair
const user = { name: "Ada", age: 36 }
const { name, age } = user
"#,
        "printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\\n' \"$first\" \"$last\" \"$sorted\" \"$reversed\" \"$head\" \"${tail[0]}\" \"$hasHttps\" \"$https\" \"$host:$port\" \"$name:$age\"",
        &[],
    );

    assert_eq!(
        stdout(output),
        "bob|carol|alice alpha|bob|carol|carol|alice alpha|bob|bob|alice alpha|true|443|host:8080|Ada:36\n"
    );
}

#[test]
fn generated_bash_runs_lambdas_and_sum_types() {
    let output = run_source(
        r#"
type Shape = Circle(Int) | Rect(Int, Int)
fn describe(shape: Shape): String {
return match shape {
Circle(radius) => "circle:${radius}",
Rect(width, height) => "rect:${width}x${height}"
}
}
fn apply(f: Int => Int, value: Int): Int {
return f(value)
}
const offset = 3
const addOffset: Int => Int = value => value + offset
const mappedValues = [1, 2, 3].map(value => value * 2)
const applied = apply(addOffset, 4)
const circle = describe(Circle(5))
const rect = describe(Rect(2, 7))
"#,
        "printf '%s,%s,%s|%s|%s|%s\\n' \"${mappedValues[0]}\" \"${mappedValues[1]}\" \"${mappedValues[2]}\" \"$applied\" \"$circle\" \"$rect\"",
        &[],
    );

    assert_eq!(stdout(output), "2,4,6|7|circle:5|rect:2x7\n");
}

#[test]
fn generated_bash_runs_match_guards_and_fallback_patterns() {
    let output = run_source(
        r#"
type Payload = Text(String) | Pair(Int, Int) | Empty
fn describe(value: Payload): String {
return match value {
Text(text) if text.len() > 0 => "text:${text}",
Pair(left, right) if left == right => "square:${left}",
Pair(left, right) => "pair:${left}:${right}",
Empty => "empty",
_ => "fallback"
}
}
const text = describe(Text("nacre"))
const square = describe(Pair(3, 3))
const pair = describe(Pair(2, 5))
const empty = describe(Empty)
const fallback = match "other" { "text" => "bad", _ => "fallback" }
"#,
        "printf '%s|%s|%s|%s|%s\\n' \"$text\" \"$square\" \"$pair\" \"$empty\" \"$fallback\"",
        &[],
    );

    assert_eq!(
        stdout(output),
        "text:nacre|square:3|pair:2:5|empty|fallback\n"
    );
}

#[test]
fn generated_bash_runs_nested_destructuring_and_collection_edges() {
    let output = run_source(
        r#"
const values = [1, 2, 3]
const [first, second, third] = values
const restValues = ["head", "middle", "tail"]
const [head, ...tail] = restValues
const user = { name: "Ada", tags: ["compiler", "math"] }
const { name, tags } = user
const endpoint = { host: "localhost", port: 8080 }
const endpointHost = endpoint.host
const endpointPort = endpoint.port
const endpointText = "${endpointHost}:${endpointPort}"
const emptyItems: [String] = []
const empty = emptyItems.isEmpty()
const missingIndex = restValues.indexOf("absent")
"#,
        "printf '%s|%s|%s|%s|%s|%s|%s|%s\\n' \"$first\" \"$second\" \"$third\" \"$head\" \"${tail[1]}\" \"$name:${tags[0]}\" \"$endpointText\" \"$empty:$missingIndex\"",
        &[],
    );

    assert_eq!(
        stdout(output),
        "1|2|3|head|tail|Ada:compiler|localhost:8080|true:-1\n"
    );
}

#[test]
fn generated_bash_runs_option_result_failure_paths_and_applicatives() {
    let output = run_source(
        r#"
fn double(value: Int): Int {
return value * 2
}
fn positive(value: Int): Int \/ String {
if value > 0 {
return Ok(value)
}
return Err("not-positive")
}
const noneValue: Int? = None
const someValue: Int? = Some(5)
const noneMapped = noneValue.map(double) ?? 99
const noneFlat = noneValue.flatMap(value => Some(value + 1)) ?? 88
const chosen = noneValue <|> someValue
const chosenValue = chosen ?? 0
const ok: Int \/ String = Ok(4)
const err: Int \/ String = Err("bad")
const errMapped = err.map(double)
const errText = match errMapped { Err(message) => message, _ => "unexpected" }
const flatErr = ok.flatMap(value => positive(value - 4))
const flatText = match flatErr { Err(message) => message, _ => "unexpected" }
const wrappedDouble: Result[Int => Int, String] = Ok(double)
const appliedOk = wrappedDouble.ap(ok) ?? 0
const appliedErr = wrappedDouble.ap(err)
const appliedErrText = match appliedErr { Err(message) => message, _ => "unexpected" }
"#,
        "printf '%s|%s|%s|%s|%s|%s|%s\\n' \"$noneMapped\" \"$noneFlat\" \"$chosenValue\" \"$errText\" \"$flatText\" \"$appliedOk\" \"$appliedErrText\"",
        &[],
    );

    assert_eq!(stdout(output), "99|88|5|bad|not-positive|8|bad\n");
}

#[test]
fn generated_bash_exposes_script_arguments() {
    let policy =
        nacre::ExecutionPolicy::from_toml("[process]\nargs = true\n", std::path::Path::new("."))
            .unwrap();
    let output = run_source_with_policy(
        r#"
const count = args.len()
const first = args[0]
const joined = args.join("|")
"#,
        &policy,
        "printf '%s|%s|%s\\n' \"$count\" \"$first\" \"$joined\"",
        &["one", "two words"],
    );

    assert_eq!(stdout(output), "2|one|one|two words\n");
}

#[test]
fn compile_file_namespaces_modules() {
    let root = temp_dir("modules");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    fs::write(
        lib.join("values.ncr"),
        r#"
fn make(value: String): String {
return "module:${value}"
}
const defaultValue = "default"
"#,
    )
    .unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use lib.values
const made = values.make("default")
const result = made
"#,
    )
    .unwrap();

    let output = run_file(&main, "printf '%s\\n' \"$result\"");
    fs::remove_dir_all(root).unwrap();
    assert_eq!(stdout(output), "module:default\n");
}

#[test]
fn compile_file_skips_policy_for_unused_imported_functions() {
    let root = temp_dir("unused-import-policy");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    fs::write(
        lib.join("tools.ncr"),
        r#"
fn unused(path: String): [String] {
return fs.readLines(path)
}
fn label(value: String): String {
return "label:${value}"
}
"#,
    )
    .unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use lib.tools
const result = tools.label("ok")
"#,
    )
    .unwrap();

    let result = nacre::compile_file_with_policy(&main, &nacre::ExecutionPolicy::deny_all());
    fs::remove_dir_all(root).unwrap();
    assert!(result.is_ok());
}

#[test]
fn compile_file_enforces_policy_for_reachable_imported_functions() {
    let root = temp_dir("reachable-import-policy");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    fs::write(
        lib.join("tools.ncr"),
        r#"
fn read(path: String): [String] {
return fs.readLines(path)
}
"#,
    )
    .unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use lib.tools
const lines = tools.read("secret.txt")
"#,
    )
    .unwrap();

    let error =
        nacre::compile_file_with_policy(&main, &nacre::ExecutionPolicy::deny_all()).unwrap_err();
    fs::remove_dir_all(root).unwrap();
    assert!(error.message().contains("fs.readLines requires"));
}

#[test]
fn compile_file_enforces_policy_for_transitive_imported_functions() {
    let root = temp_dir("transitive-import-policy");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    fs::write(
        lib.join("tools.ncr"),
        r#"
fn read(path: String): [String] {
return fs.readLines(path)
}
fn wrapper(path: String): [String] {
return read(path)
}
"#,
    )
    .unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use lib.tools
const lines = tools.wrapper("secret.txt")
"#,
    )
    .unwrap();

    let error =
        nacre::compile_file_with_policy(&main, &nacre::ExecutionPolicy::deny_all()).unwrap_err();
    fs::remove_dir_all(root).unwrap();
    assert!(error.message().contains("fs.readLines requires"));
}

#[test]
fn compile_file_enforces_policy_for_imported_function_values() {
    let root = temp_dir("function-value-import-policy");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    fs::write(
        lib.join("tools.ncr"),
        r#"
fn read(path: String): [String] {
return fs.readLines(path)
}
"#,
    )
    .unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use lib.tools
const reader: String => [String] = tools.read
"#,
    )
    .unwrap();

    let error =
        nacre::compile_file_with_policy(&main, &nacre::ExecutionPolicy::deny_all()).unwrap_err();
    fs::remove_dir_all(root).unwrap();
    assert!(error.message().contains("fs.readLines requires"));
}

#[test]
fn compile_file_enforces_policy_for_imported_top_level_initializers() {
    let root = temp_dir("import-top-level-policy");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    fs::write(
        lib.join("tools.ncr"),
        r#"
const lines = fs.readLines("secret.txt")
fn label(value: String): String {
return value
}
"#,
    )
    .unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use lib.tools
const result = tools.label("ok")
"#,
    )
    .unwrap();

    let error =
        nacre::compile_file_with_policy(&main, &nacre::ExecutionPolicy::deny_all()).unwrap_err();
    fs::remove_dir_all(root).unwrap();
    assert!(error.message().contains("fs.readLines requires"));
}

#[test]
fn compile_file_resolves_pure_standard_modules() {
    let root = temp_dir("std");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.path
use std.str
const base = path.basename("/tmp/nacre.txt")
const dir = path.dirname("/tmp/nacre.txt")
const stem = path.stem("/tmp/nacre.txt")
const ext = path.extname("/tmp/nacre.txt")
const clean = str.trim(" safe ")
"#,
    )
    .unwrap();

    let output = run_file(
        &main,
        "printf '%s|%s|%s|%s|%s\\n' \"$base\" \"$dir\" \"$stem\" \"$ext\" \"$clean\"",
    );
    fs::remove_dir_all(root).unwrap();
    assert_eq!(stdout(output), "nacre.txt|/tmp|nacre|.txt|safe\n");
}

#[test]
fn unsafe_shell_constructs_are_rejected_by_public_apis() {
    for source in [
        r#"$sh"echo unsafe""#,
        "raw {\necho unsafe\n}\n",
        r#"require("git")"#,
        r#"const found = hasCommand("git")"#,
        r#"const value = $sh"printf unsafe" |> $sh"cat""#,
    ] {
        let error = nacre::compile_source(source).unwrap_err();
        assert!(
            error.message().contains("disabled"),
            "source:\n{source}\nerror: {error}"
        );
    }
}
