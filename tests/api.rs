use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn public_api_accessors_are_covered_from_integration_tests() {
    let program = nacre::parse("const answer = 42\n").unwrap();
    assert_eq!(program.statements().len(), 1);

    let error = nacre::compile_source("const bad-name = 1").unwrap_err();
    assert_eq!(error.line(), 1);
    assert!(error.message().contains("invalid variable name"));
}

#[test]
fn public_api_type_check_covers_supported_statement_paths() {
    let program = nacre::parse(
        r#"
use lib.utils
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
fn run(): Unit {
const names = ["a", "b"]
let mutableNames = ["a"]
mutableNames.push("b")
mutableNames.pop()
let count = 2
count = count - 1
choose(count)
try $sh"true"
$sh"true"
$sh"printf x" >> write("/tmp/nacre-type-check")
require("git", version = ">= 1")
requireOneOf(["git", "sh"])
const argCount = args.len()
if count > 0 {
const label = "ok"
} else {
const label = "no"
}
if count > 0 {
const onlyThen = "ok"
}
while count > 0 {
count = count - 1
break
}
for name in names {
continue
}
raw {
echo raw
}
}
const uid: UserId = UserId(1)
const rawId: Int = uid.value
const shown = Show.show(1)
let mutable: Id[Int] = 1
mutable = 2
run()
"#,
    )
    .unwrap();

    nacre::type_check(&program).unwrap();
}

#[test]
fn public_api_type_check_covers_generic_structures() {
    let program = nacre::parse(
        r#"
fn firstArray[T](value: [T]): T {
return value[0]
}
fn mapValue[T](value: Map[String, T]): T {
return value["k"]
}
fn tupleFirst[T](value: (T, String)): T {
return value._1
}
fn recordItem[T](value: { item: T }): T {
return value.item
}
fn identityInt(value: Int): Int {
return value
}
fn applySame[T](f: T => T, value: T): T {
return f(value)
}
const a = firstArray([1, 2])
const b = mapValue({ "k": 3 })
const c = tupleFirst((4, "x"))
const d = recordItem({ item: 5 })
const e = applySame(identityInt, 6)
"#,
    )
    .unwrap();

    nacre::type_check(&program).unwrap();
}

#[test]
fn public_api_type_check_covers_union_and_intersection_types() {
    let program = nacre::parse(
        r#"
const textOrInt: String | Int = "value"
const numberOrText: String | Int = 42
const stringPath: String & Path = "/tmp/nacre"
fn identityUnion[T](value: T | String): T | String {
return value
}
const inferred = identityUnion(7)
"#,
    )
    .unwrap();

    nacre::type_check(&program).unwrap();
}

#[test]
fn public_api_type_check_covers_error_paths() {
    let cases = [
        ("trait Show[T] {\nfn show(value: T): String\n}\ntrait Show[T] {\n}", "trait `Show` is already defined"),
        ("trait Show[T] {\nfn show(value: T): String\nfn show(value: T): String\n}", "trait method `show` is already defined"),
        ("trait Show[T] {\nfn show(value: String): String\n}", "receiver must be `T`"),
        ("impl Show[Int] {\n}", "unknown trait `Show`"),
        ("type Box[T] = { item: T }\ntype Box[T] = { item: T }", "type `Box` is already defined"),
        ("type Alias = Int\ntype Alias[T] = T", "type `Alias` is already defined"),
        ("fn greet(value: String): String {\nreturn value\n}\nfn greet(value: String): String {\nreturn value\n}", "function `greet` is already defined"),
        ("const greet = \"x\"\nfn greet(value: String): String {\nreturn value\n}", "conflicts with existing variable"),
        ("fn greet(value: String = \"x\", suffix: String): String {\nreturn value\n}", "required function parameters cannot follow default parameters"),
        ("fn greet(value: String = 1): String {\nreturn value\n}", "default for parameter `value`"),
        ("fn greet[T: Missing](value: T): T {\nreturn value\n}", "unknown trait `Missing`"),
        ("const x: ExitCode = 300", "type annotation mismatch"),
        ("let x = 1\nx = \"s\"", "type mismatch"),
        ("const x = 1\nx = 2", "cannot assign to const"),
        ("x = 1", "cannot assign to undefined variable"),
        ("if 1 {\nconst x = 1\n}", "condition must be Bool"),
        ("while 1 {\nconst x = 1\n}", "condition must be Bool"),
        ("const x = 1\nfor item in x {\nconst y = item\n}", "for loop iterable must be Array"),
        ("return 1", "return is only valid inside a function"),
    ];

    for (source, message) in cases {
        let program = nacre::parse(source).unwrap();
        let error = nacre::type_check(&program).unwrap_err();
        assert!(error.message().contains(message), "{error}");
    }
}

#[test]
fn public_api_covers_supported_success_paths() {
    let bash = nacre::compile_source(
        r#"
## ignored
#! /usr/bin/env nacre
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
trait Debug[T] {
fn show(value: T): String
}
impl Debug[Int] {
fn show(value: Int): String {
return "debug ${value}"
}
}
fn boundedIdentity[T: Show](value: T): T {
return value
}
const boundedInt = boundedIdentity(7)
const shownInt = Show.show(boundedInt)
const debugInt = Debug.show(boundedInt)
const shownText = genericText.show()
type Box[T] = { item: T }
const boxed: Box[Int] = { item: 7 }
const boxedValue = boxed.item
const future = async $sh"printf async"
const asyncOut = await future
const spawned = spawn $sh"printf spawned"
const spawnedOut = spawned.wait()
const status = if answer > 0 { "positive" } else { "zero" }
const matched = match status { "positive" => "matched", _ => "fallback" }
let nums = [1, 2, 3]
const hasTwo = nums.contains(2)
const secondIndex = nums.indexOf(2)
const missingIndex = nums.indexOf(9)
nums = [4, 5]
const envs: Map[String, String] = { "PORT": "8080", "HOST": "localhost" }
const envCount = envs.len()
const envsEmpty = envs.isEmpty()
const envKeys = envs.keys()
const envValues = envs.values()
const hasPort = envs.has("PORT")
const arrayKeyMap = { ["a"]: 1 }
const tupleKeyMap = { ("a", "b"): 1 }
const recordKeyMap = { { name: "a" }: 1 }
const port = envs["PORT"]
const firstName = names[0]
const nameCount = names.len()
const namesEmpty = names.isEmpty()
const nameList = names.join(",")
const hasAlice = names.contains("alice")
const firstNames = names.slice(0, 1)
const csv = "alice,bob"
const csvParts = csv.split(",")
const csvList = csvParts.join("|")
const renamed = csv.replace("alice", "ada")
const pair: (String, Int) = ("localhost", 8080)
const hostName = pair._1
const portNumber = pair._2
const user: { name: String, age: Int } = { name: "Ada", age: 36 }
const userName = user.name
const userAge = user.age
type Account = { id: Int, name: String }
const account: Account = { id: 1, name: "core" }
const accountName = account.name
newtype UserId = Int
newtype GroupId = Int
const uid: UserId = UserId(42)
const rawUid: Int = uid.value
const copied = answer
const text = "a'b"
const greeting = "Hello, ${text}"
const rawText = r"keep \n raw"
const joinedText = "join" ++ "ed"
const shell = env.SHELL
const home = env.HOME ?? "/tmp"
const host = $sh"printf api"
const requiredHost = try $sh"printf required"
const piped = $sh"printf pipeline" |> $sh"cat"
const braced = $sh{ printf '%s' "braced api" }
const bracedPipe = $sh{ printf braced } |> $sh{ cat }
const hasGit = hasCommand("git")
let count = 10
count = count - 2 / 1 % 2
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
const envCmp = env.PATH ?? "/bin" == "/bin"
try $sh"true"
try $sh"true" |> $sh"cat"
$sh'echo ok'
require("git", version = ">= 1")
requireOneOf(["curl", "wget"])
if count > 0 {
$sh'echo positive'
} else {
$sh'echo zero'
}
while count > 0 {
if count == 1 {
break
}
count = count - 1
continue
}
for person in names {
$sh"echo ${person}"
}
raw {
echo raw
}
"#,
    )
    .unwrap();

    assert!(bash.contains("readonly yes=true"));
    assert!(bash.contains("args=(\"$@\")"));
    assert!(bash.contains("readonly hex=255"));
    assert!(bash.contains("readonly bits=10"));
    assert!(bash.contains("readonly pi=3.14"));
    assert!(bash.contains("readonly unit=''"));
    assert!(bash.contains("readonly bin='/usr/bin'"));
    assert!(bash.contains("greet() {\nlocal __nacre_local_greet_0_name=\"$1\""));
    assert!(bash.contains("readonly greet='greet'"));
    assert!(bash.contains("readonly message=\"$(__nacre_call \"$greet\" 'Nacre')\""));
    assert!(bash.contains("exclaim() {\nlocal __nacre_local_exclaim_0_value=\"$1\""));
    assert!(bash.contains("readonly exclaim='exclaim'"));
    assert!(bash.contains("applyString() {\nlocal __nacre_local_applyString_0_f=\"$1\""));
    assert!(
        bash.contains("readonly applied=\"$(__nacre_call \"$applyString\" \"$exclaim\" 'Hi')\"")
    );
    assert!(bash.contains("identity() {\nlocal __nacre_local_identity_0_value=\"$1\""));
    assert!(bash.contains("readonly genericText=\"$(__nacre_call \"$identity\" 'generic')\""));
    assert!(bash.contains("readonly genericInt=\"$(__nacre_call \"$identity\" 7)\""));
    assert!(
        bash.contains("readonly methodApplied=\"$(__nacre_call \"$exclaim\" \"$genericText\")\"")
    );
    assert!(bash.contains(
        "__nacre_trait_Show_Int_show() {\nlocal __nacre_local___nacre_trait_Show_Int_show_0_value=\"$1\""
    ));
    assert!(bash.contains(
        "__nacre_trait_Show_String_show() {\nlocal __nacre_local___nacre_trait_Show_String_show_0_value=\"$1\""
    ));
    assert!(
        bash.contains("boundedIdentity() {\nlocal __nacre_local_boundedIdentity_0_value=\"$1\"")
    );
    assert!(bash.contains("readonly boundedInt=\"$(__nacre_call \"$boundedIdentity\" 7)\""));
    assert!(bash.contains(
        "readonly shownInt=\"$(__nacre_call \"$__nacre_trait_Show_Int_show\" \"$boundedInt\")\""
    ));
    assert!(bash.contains(
        "readonly debugInt=\"$(__nacre_call \"$__nacre_trait_Debug_Int_show\" \"$boundedInt\")\""
    ));
    assert!(bash.contains(
        "readonly shownText=\"$(__nacre_call \"$__nacre_trait_Show_String_show\" \"$genericText\")\""
    ));
    assert!(bash.contains("readonly boxed_item=7"));
    assert!(bash.contains("readonly boxedValue=\"$boxed_item\""));
    assert!(bash.contains("future_out=\"$(mktemp)\""));
    assert!(bash.contains("printf async > \"$future_out\" 2>&1 &"));
    assert!(bash.contains("if wait \"$future_pid\"; then"));
    assert!(bash.contains("readonly asyncOut"));
    assert!(bash.contains("spawned_out=\"$(mktemp)\""));
    assert!(bash.contains("printf spawned > \"$spawned_out\" 2>&1 &"));
    assert!(bash.contains("if wait \"$spawned_pid\"; then"));
    assert!(bash.contains("readonly spawnedOut"));
    assert!(bash.contains("readonly status=$(if awk -v __nacre_0=\"$answer\""));
    assert!(
        bash.contains("readonly matched=\"$(__nacre_match=\"$status\"; if case \"$__nacre_match\"")
    );
    assert!(bash.contains("readonly -a names=('alice' 'bob')"));
    assert!(bash.contains("nums=(1 2 3)"));
    assert!(bash.contains("nums=(4 5)"));
    assert!(bash.contains("declare -Ar envs=(['PORT']='8080' ['HOST']='localhost')"));
    assert!(bash.contains("readonly envCount=\"${#envs[@]}\""));
    assert!(bash.contains("readonly envsEmpty=$(if [ \"${#envs[@]}\" -eq 0 ]; then printf true; else printf false; fi)"));
    assert!(bash.contains("readonly -a envKeys=(\"${!envs[@]}\")"));
    assert!(bash.contains("readonly -a envValues=(\"${envs[@]}\")"));
    assert!(bash.contains(
        "readonly hasPort=$(if [[ -v envs['PORT'] ]]; then printf true; else printf false; fi)"
    ));
    assert!(bash.contains("readonly port=\"${envs['PORT']}\""));
    assert!(bash.contains("readonly firstName=\"${names[0]}\""));
    assert!(bash.contains("readonly nameCount=\"${#names[@]}\""));
    assert!(bash.contains("readonly namesEmpty=$(if [ \"${#names[@]}\" -eq 0 ]; then printf true; else printf false; fi)"));
    assert!(bash.contains("readonly nameList=\"$(__nacre_join_first=true; for __nacre_join_item in \"${names[@]}\"; do if [ \"$__nacre_join_first\" = true ]; then __nacre_join_first=false; else printf '%s' ','; fi; printf '%s' \"$__nacre_join_item\"; done)\""));
    assert!(bash.contains("readonly hasAlice=$(__nacre_contains=false; for __nacre_item in \"${names[@]}\"; do if [ \"$__nacre_item\" = 'alice' ]; then __nacre_contains=true; break; fi; done; printf '%s' \"$__nacre_contains\")"));
    assert!(bash.contains("readonly -a firstNames=(\"${names[@]:$((0)):$((1 - 0))}\")"));
    assert!(bash.contains("mapfile -t csvParts < <(awk -v __nacre_value=\"$csv\""));
    assert!(bash.contains("readonly -a csvParts"));
    assert!(bash.contains(
        "readonly csvList=\"$(__nacre_join_first=true; for __nacre_join_item in \"${csvParts[@]}\""
    ));
    assert!(bash.contains("readonly renamed=\"$(awk -v __nacre_value=\"$csv\""));
    assert!(bash.contains("-v __nacre_from='alice' -v __nacre_to='ada'"));
    assert!(bash.contains("readonly pair_1='localhost'"));
    assert!(bash.contains("readonly pair_2=8080"));
    assert!(bash.contains("readonly hostName=\"$pair_1\""));
    assert!(bash.contains("readonly portNumber=\"$pair_2\""));
    assert!(bash.contains("readonly user_name='Ada'"));
    assert!(bash.contains("readonly user_age=36"));
    assert!(bash.contains("readonly userName=\"$user_name\""));
    assert!(bash.contains("readonly userAge=\"$user_age\""));
    assert!(bash.contains("readonly account_id=1"));
    assert!(bash.contains("readonly account_name='core'"));
    assert!(bash.contains("readonly accountName=\"$account_name\""));
    assert!(bash.contains("readonly uid=42"));
    assert!(bash.contains("readonly rawUid=\"$uid\""));
    assert!(bash.contains("readonly copied=\"$answer\""));
    assert!(bash.contains("readonly text='a'\\''b'"));
    assert!(bash.contains("readonly greeting=\"Hello, ${text}\""));
    assert!(bash.contains("readonly rawText='keep \\n raw'"));
    assert!(bash.contains("readonly joinedText=\"$(printf '%s' 'join' 'ed')\""));
    assert!(bash.contains("readonly shell=\"${SHELL}\""));
    assert!(bash.contains("readonly host=\"$(printf api)\""));
    assert!(bash.contains("requiredHost=\"$(printf required)\" || exit $?\nreadonly requiredHost"));
    assert!(bash.contains("readonly piped=\"$(printf pipeline | cat)\""));
    assert!(bash.contains("readonly braced=\"$(printf '%s' \"braced api\")\""));
    assert!(bash.contains("readonly bracedPipe=\"$(printf braced | cat)\""));
    assert!(bash.contains(
        "readonly hasGit=$(command -v 'git' >/dev/null 2>&1 && printf true || printf false)"
    ));
    assert!(bash.contains("count=$(awk -v __nacre_0=\"$count\""));
    assert!(bash.contains("widened=\"$answer\""));
    assert!(bash.contains("readonly ne=$(awk -v __nacre_0=\"$answer\""));
    assert!(bash.contains("readonly floatCmp=$(awk -v __nacre_0=\"$pi\""));
    assert!(bash.contains("readonly logical=$(awk "));
    assert!(bash.contains(" && "));
    assert!(bash.contains(" || "));
    assert!(bash.contains("readonly envCmp=$(awk -v __nacre_0=\"${PATH:-/bin}\""));
    assert!(bash.contains("true || exit $?"));
    assert!(bash.contains("command -v 'git' >/dev/null 2>&1"));
    assert!(bash.contains("command -v 'curl' >/dev/null 2>&1 || command -v 'wget' >/dev/null 2>&1"));
    assert!(bash.contains(
        "if awk -v __nacre_0=\"$count\" 'BEGIN { exit (((__nacre_0 > 0)) ? 0 : 1) }'; then\necho positive\nelse\necho zero\nfi"
    ));
    assert!(bash.contains(
        "while awk -v __nacre_0=\"$count\" 'BEGIN { exit (((__nacre_0 > 0)) ? 0 : 1) }'; do\nif awk -v __nacre_0=\"$count\" 'BEGIN { exit (((__nacre_0 == 1)) ? 0 : 1) }'; then\nbreak\nfi\ncount=$(awk -v __nacre_0=\"$count\" 'BEGIN { print ((__nacre_0 - 1)) }')\ncontinue\ndone"
    ));
    assert!(bash.contains("for person in \"${names[@]}\"; do\necho ${person}\ndone"));
    assert!(bash.contains("echo raw"));
}

#[test]
fn public_api_compile_source_emits_use_statements() {
    let bash = nacre::compile_source("use lib.utils\n").unwrap();

    assert!(bash.contains("source \"$(dirname \"$0\")/lib/utils.sh\""));
}

#[test]
fn generated_bash_discards_underscore_bindings() {
    let source = r#"
fn value(): String {
return "hidden"
}
const _ = try $sh"printf const-hidden"
_ = $sh"printf assign-hidden"
const _ = value()
const _ = 1 + 2
try $sh"printf visible"
"#;
    let bash = nacre::compile_source(&source).unwrap();
    let script = temp_path("discard-underscore.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(!bash.contains("readonly _"));
    assert!(!bash.contains("\n_="));
    assert!(bash.contains("printf const-hidden >/dev/null || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "visible");
}

#[test]
fn generated_bash_runs_comparisons_and_float_arithmetic() {
    let redirect = temp_path("redirect.txt");
    let source = r#"
const pi = 3.14
const doubled = pi * 2
const large = doubled > 6
const text = "a'b"
const multi = """
line one
line "two"
"""
const same = text == "a'b"
const combined = "na" ++ "cre"
const combinedEq = combined == "nacre"
const combinedLen = combined.len()
const combinedEmpty = combined.isEmpty()
const combinedSlice = combined.slice(1, 4)
const blank = ""
const blankEmpty = blank.isEmpty()
const containsText = combined.contains("ac")
const acIndex = combined.indexOf("ac")
const missingTextIndex = combined.indexOf("zz")
const padded = "  nacre  "
const trimmed = padded.trim()
const leftTrimmed = padded.trimStart()
const rightTrimmed = padded.trimEnd()
const repeated = combined.repeat(3)
const repeatedEmpty = combined.repeat(0)
const csv = "left,space value,right"
const csvParts = csv.split(",")
const splitList = csvParts.join("|")
const literalJoined = (["left", "right"]).join("|")
const literalLen = (["left", "right"]).len()
const literalEmpty = ([]).isEmpty()
const literalIndexed = (["left", "right"])[1]
const literalFirst = (["left", "right"]).first()
const literalLast = (["left", "right"]).last()
const literalHas = (["left", "space value", "right"]).contains("space value")
const literalIndex = (["left", "space value", "right"]).indexOf("space value")
const literalMissingIndex = (["left", "space value", "right"]).indexOf("missing")
const literalReversed = (["left", "space value", "right"]).reverse()
const literalSorted = (["right", "left", "space value"]).sort()
const literalUnique = (["left", "left", "space value"]).unique()
const literalReversedList = literalReversed.join("|")
const literalSortedList = literalSorted.join("|")
const literalUniqueList = literalUnique.join("|")
const literalSlice = (["left", "space value", "right"]).slice(1, 3)
const literalTake = (["left", "space value", "right"]).take(2)
const literalDrop = (["left", "space value", "right"]).drop(1)
const literalSliceList = literalSlice.join("|")
const literalTakeList = literalTake.join("|")
const literalDropList = literalDrop.join("|")
const replaced = csv.replace("space value", "center")
let splitLoop = ""
for part in csv.split(",") {
splitLoop = splitLoop ++ part ++ ";"
}
const both = large && same
const either = false || both
const inverted = !either
const masked = 6 & 3
const bits = masked | 8
const flipped = bits ^ 1
const shifted = flipped << 1
const restored = shifted >> 1
const bitInverse = ~restored
const bitCheck = bits & 2 == 2
const grouped = (1 + 2) * 3
newtype CastId = Int
const castRaw = 9
const castId: CastId = castRaw as CastId
const castBack: Int = castId as Int
const castPath: Path = "/tmp"
const castText: String = castPath as String
const words = ["left", "middle", "right"]
const emptyWords: [String] = []
const envs: Map[String, String] = { "PORT": "8080", "HOST": "localhost" }
const singleEnv: Map[String, String] = { "ONLY": "one" }
const [head, ...tail] = words
const wordList = words.join("::")
const middleWords = words.slice(1, 3)
const wordsEmpty = emptyWords.isEmpty()
const envCount = envs.len()
const envsEmpty = envs.isEmpty()
const literalMapLen = ({ "LIT": "literal" }).len()
const literalMapEmpty = ({}).isEmpty()
const literalMapIndexed = ({ "LIT": "literal" })["LIT"]
const hasPort = envs.has("PORT")
const singleKeys = singleEnv.keys()
const singleValues = singleEnv.values()
const literalKeys = ({ "LIT": "literal" }).keys()
const literalValues = ({ "LIT": "literal" }).values()
const literalMapHas = ({ "LIT": "literal" }).has("LIT")
const label = if large { "big" } else { "small" }
const matched = match label { "big" => "matched", _ => "fallback" }
const piped = $sh"printf pipeline" |> $sh"cat"
const braced = $sh{ printf '%s' "braced run" }
const bracedPipe = $sh{ printf braced } |> $sh{ cat }
require("bash", version = ">= 1")
if large {
try $sh"echo ${doubled} ${same}"
}
try $sh"echo ${both} ${either} ${inverted}"
try $sh"echo ${combined} ${combinedEq} ${combinedLen} ${combinedEmpty} ${blankEmpty} ${containsText} ${acIndex} ${missingTextIndex} ${combinedSlice} ${trimmed}"
try $sh"printf '%s|%s\n' \"${leftTrimmed}\" \"${rightTrimmed}\""
try $sh"printf '%s|%s\n' \"${repeated}\" \"${repeatedEmpty}\""
try $sh"echo ${splitList} ${literalJoined} ${literalLen} ${literalEmpty} ${literalIndexed} ${literalFirst} ${literalLast} ${literalHas} ${literalIndex} ${literalMissingIndex} ${literalReversedList} ${literalSortedList} ${literalUniqueList} ${literalSliceList} ${literalTakeList} ${literalDropList} ${splitLoop} ${replaced}"
try $sh"echo ${head} ${tail[0]} ${tail[1]}"
try $sh"echo ${wordList} ${middleWords[0]} ${middleWords[1]} ${wordsEmpty} ${envCount} ${envsEmpty} ${literalMapLen} ${literalMapEmpty} ${literalMapIndexed} ${hasPort} ${singleKeys[0]} ${singleValues[0]} ${literalKeys[0]} ${literalValues[0]} ${literalMapHas}"
try $sh'printf "%s" "${multi}"'
try $sh"echo ${bits} ${shifted} ${restored} ${bitInverse} ${bitCheck} ${grouped}"
try $sh"echo ${castBack} ${castText}"
fn greet(name: String, prefix: String = "Hello"): String {
return "${prefix}, ${name}"
}
const message = greet("Nacre", "Hi")
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
type Box[T] = { item: T }
const boxed: Box[Int] = { item: 7 }
const boxedValue = boxed.item
const pair: (String, Int) = ("host", 8080)
const literalTupleHost = ("literal-host", 9090)._1
const (tupleHost, tuplePort) = pair
const user: { name: String, age: Int } = { name: "Ada", age: 36 }
const literalUserName = ({ name: "Grace", age: 37 }).name
const { age } = user
const future = async $sh"printf async"
const asyncOut = await future
try $sh"echo ${label}"
try $sh"echo ${matched}"
try $sh"echo ${piped}"
try $sh"echo ${braced}"
try $sh"echo ${bracedPipe}"
try $sh"echo ${message}"
try $sh"echo ${applied}"
try $sh"echo ${summary}"
try $sh"echo ${methodApplied}"
try $sh"echo ${genericText} ${genericInt}"
try $sh"echo ${boxedValue}"
try $sh"echo ${tupleHost} ${tuplePort} ${literalTupleHost} ${literalUserName} ${age}"
try $sh"echo ${asyncOut}"
$sh"printf write" >> write("__REDIRECT__")
$sh"printf append" >> append("__REDIRECT__")
"#
    .replace("__REDIRECT__", &redirect.display().to_string());
    let bash = nacre::compile_source(&source).unwrap();
    let script = temp_path("comparisons-floats.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "6.28 true\ntrue true false\nnacre true 5 false true true 1 -1 acr nacre\nnacre  |  nacre\nnacrenacrenacre|\nleft|space value|right left|right 2 true right left right true 1 -1 right|space value|left left|right|space value left|space value space value|right left|space value space value|right left;space value;right; left,center,right\nleft middle right\nleft::middle::right middle right true 2 false 1 true literal true ONLY one LIT literal true\n\nline one\nline \"two\"\n10 22 11 -12 true 9\n9 /tmp\nbig\nmatched\npipeline\nbraced run\nbraced\nHi, Nacre\nHi!\nrest:one:3\ngeneric!\ngeneric 7\n7\nhost 8080 literal-host Grace 36\nasync\n"
    );
    assert_eq!(fs::read_to_string(&redirect).unwrap(), "writeappend");
    fs::remove_file(redirect).unwrap();
}

#[test]
fn generated_bash_runs_option_defaults() {
    let source = r#"
fn fallback(value: String?): String {
return value ?? "fallback"
}
const present: String? = Some("Ada Lovelace")
const missing: String? = None
const presentValue = present ?? "missing"
const missingValue = missing ?? "empty"
const directInt = Some(7) ?? 0
const fallbackValue = fallback(None)
try $sh"printf '%s|%s|%s|%s\n' \"${presentValue}\" \"${missingValue}\" \"${directInt}\" \"${fallbackValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("option-defaults.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("printf '1%s' 'Ada Lovelace'"));
    assert!(bash.contains("readonly missing='0'"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Ada Lovelace|empty|7|fallback\n"
    );
}

#[test]
fn generated_bash_maps_options_with_lambdas_and_functions() {
    let source = r#"
fn length(value: String): Int {
return value.len()
}
fn mapLocal(value: String?): Int? {
return value.map(item => item.len() + 1)
}
const present: String? = Some("Ada")
const missing: String? = None
const upper = present.map(value => value.toUpper())
const stillMissing = missing.map(value => value.toUpper())
const direct = Some(7).map(value => value * 3)
const named = present.map(length)
const localPresent = mapLocal(Some("four"))
const localMissing = mapLocal(None)
const upperValue = upper ?? "missing"
const missingValue = stillMissing ?? "empty"
const directValue = direct ?? 0
const namedValue = named ?? 0
const localPresentValue = localPresent ?? 0
const localMissingValue = localMissing ?? 8
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${upperValue}\" \"${missingValue}\" \"${directValue}\" \"${namedValue}\" \"${localPresentValue}\" \"${localMissingValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("option-map.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("case \"$__nacre_option\" in 1*)"));
    assert!(bash.contains("*) printf '0' ;; esac"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ADA|empty|21|3|5|8\n"
    );
}

#[test]
fn generated_bash_flat_maps_options_and_short_circuits_none() {
    let source = r#"
fn keepLong(value: String): Int? {
if value.len() > 2 {
return Some(value.len())
}
return None
}
fn failIfCalled(value: String): String? {
$sh"false"
return Some(value)
}
fn flatMapLocal(value: Int?): Int? {
return value.flatMap(item => Some(item * 4))
}
const present: String? = Some("Ada")
const short: String? = Some("x")
const missing: String? = None
const upper = present.flatMap(value => Some(value.toUpper()))
const rejected = short.flatMap(keepLong)
const length = present.flatMap(keepLong)
const direct = Some(3).flatMap(value => Some(value + 2))
const stillMissing = missing.flatMap(failIfCalled)
const localPresent = flatMapLocal(Some(2))
const localMissing = flatMapLocal(None)
const upperValue = upper ?? "missing"
const rejectedValue = rejected ?? 7
const lengthValue = length ?? 0
const directValue = direct ?? 0
const missingValue = stillMissing ?? "empty"
const localPresentValue = localPresent ?? 0
const localMissingValue = localMissing ?? 9
try $sh"printf '%s|%s|%s|%s|%s|%s|%s\n' \"${upperValue}\" \"${rejectedValue}\" \"${lengthValue}\" \"${directValue}\" \"${missingValue}\" \"${localPresentValue}\" \"${localMissingValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("option-flat-map.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("case \"$__nacre_option\" in 1*)"));
    assert!(bash.contains("*) printf '0' ;; esac"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ADA|7|3|5|empty|8|9\n"
    );
}

#[test]
fn generated_bash_uses_option_or_else_and_alternative_operator() {
    let source = r#"
fn fallback(): String? {
return Some("fallback")
}
fn failIfCalled(): String? {
$sh"false"
return Some("bad")
}
const present: String? = Some("ready")
const missing: String? = None
const methodPresent = present.orElse(failIfCalled())
const methodMissing = missing.orElse(fallback())
const operatorPresent = present <|> failIfCalled()
const operatorMissing = missing <|> Some("operator")
const literal = None <|> Some("literal")
const unwrapped = missing <|> Some("chosen") ?? "empty"
const methodPresentValue = methodPresent ?? "missing"
const methodMissingValue = methodMissing ?? "missing"
const operatorPresentValue = operatorPresent ?? "missing"
const operatorMissingValue = operatorMissing ?? "missing"
const literalValue = literal ?? "missing"
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${methodPresentValue}\" \"${methodMissingValue}\" \"${operatorPresentValue}\" \"${operatorMissingValue}\" \"${literalValue}\" \"${unwrapped}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("option-or-else.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("case \"$__nacre_option\" in 1*)"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ready|fallback|ready|operator|literal|chosen\n"
    );
}

#[test]
fn generated_bash_runs_map_and_flat_map_operator_aliases() {
    let source = r#"
fn failInt(value: Int): Int {
$sh"false"
return value
}
fn failOption(value: Int): Int? {
$sh"false"
return Some(value)
}
fn failResult(value: Int): Int \/ String {
$sh"false"
return Ok(value)
}
const numbers = [1, 2, 3]
const doubled = numbers <$> (value => value * 2)
const present: Int? = Some(4)
const missing: Int? = None
const optionMapped = present <$> (value => value + 1)
const optionFlat = present >>= (value => Some(value * 3))
const missingMapped = missing <$> failInt
const missingFlat = missing >>= failOption
const ok: Int \/ String = Ok(5)
const err: Int \/ String = Err("original")
const resultMapped = ok <$> (value => value + 2)
const resultFlat = ok >>= (value => Ok(value * 2))
const errMapped = err <$> failInt
const errFlat = err >>= failResult
const optionMappedValue = optionMapped ?? 0
const optionFlatValue = optionFlat ?? 0
const missingMappedValue = missingMapped ?? 8
const missingFlatValue = missingFlat ?? 9
const resultMappedValue = resultMapped ?? 0
const resultFlatValue = resultFlat ?? 0
const errMappedError = match errMapped { Err(error) => error, _ => "missing" }
const errFlatError = match errFlat { Err(error) => error, _ => "missing" }
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${doubled[0]}\" \"${doubled[2]}\" \"${optionMappedValue}\" \"${optionFlatValue}\" \"${missingMappedValue}\" \"${missingFlatValue}\" \"${resultMappedValue}\" \"${resultFlatValue}\" \"${errMappedError}\" \"${errFlatError}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("map-flat-map-operators.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "2|6|5|12|8|9|7|10|original|original\n"
    );
}

#[test]
fn generated_bash_applies_wrapped_functions_and_short_circuits() {
    let source = r#"
fn double(value: Int): Int {
return value * 2
}
fn failInt(value: Int): Int {
$sh"false"
return value
}
fn failOption(): Int? {
$sh"false"
return Some(9)
}
fn failResult(): Int \/ String {
$sh"false"
return Ok(9)
}
const optionFunction: Option[Int => Int] = Some(double)
const optionValue: Int? = Some(3)
const optionMethod = optionFunction.ap(optionValue)
const optionAlias = optionFunction <*> optionValue
const missingFunction: Option[Int => Int] = None
const missingValue: Int? = None
const skippedOptionValue = missingFunction.ap(failOption())
const skippedOptionFunction = Some(failInt).ap(missingValue)
const resultFunction: Result[Int => Int, String] = Ok(double)
const resultValue: Int \/ String = Ok(4)
const resultMethod = resultFunction.ap(resultValue)
const resultAlias = resultFunction <*> resultValue
const failedFunction: Result[Int => Int, String] = Err("function-error")
const failedValue: Int \/ String = Err("value-error")
const skippedResultValue = failedFunction.ap(failResult())
const skippedResultFunction = Ok(failInt).ap(failedValue)
const optionMethodValue = optionMethod ?? 0
const optionAliasValue = optionAlias ?? 0
const missingFunctionValue = skippedOptionValue ?? 7
const missingValueValue = skippedOptionFunction ?? 8
const resultMethodValue = resultMethod ?? 0
const resultAliasValue = resultAlias ?? 0
const functionError = match skippedResultValue { Err(error) => error, _ => "missing" }
const valueError = match skippedResultFunction { Err(error) => error, _ => "missing" }
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s\n' \"${optionMethodValue}\" \"${optionAliasValue}\" \"${missingFunctionValue}\" \"${missingValueValue}\" \"${resultMethodValue}\" \"${resultAliasValue}\" \"${functionError}\" \"${valueError}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("applicative-ap.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "6|6|7|8|8|8|function-error|value-error\n"
    );
}

#[test]
fn generated_bash_runs_result_defaults() {
    let source = r#"
fn fallback(value: String \/ String): String {
return value ?? "fallback"
}
const ok: String \/ String = Ok("ready")
const err: String \/ String = Err("failed")
const okValue = ok ?? "missing"
const errValue = err ?? "recovered"
const directInt = Ok(9) ?? 0
const fallbackValue = fallback(Err("nope"))
try $sh"printf '%s|%s|%s|%s\n' \"${okValue}\" \"${errValue}\" \"${directInt}\" \"${fallbackValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("result-defaults.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("printf '1%s' 'ready'"));
    assert!(bash.contains("printf '0%s' 'failed'"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ready|recovered|9|fallback\n"
    );
}

#[test]
fn generated_bash_maps_and_flat_maps_results_preserving_errors() {
    let source = r#"
fn length(value: String): Int {
return value.len()
}
fn requireLong(value: String): Int \/ String {
if value.len() > 2 {
return Ok(value.len())
}
return Err("too-short")
}
fn failIfCalled(value: String): String {
$sh"false"
return value
}
fn failResultIfCalled(value: String): String \/ String {
$sh"false"
return Ok(value)
}
fn mapLocal(value: Int \/ String): Int \/ String {
return value.map(item => item * 3)
}
fn flatMapLocal(value: Int \/ String): Int \/ String {
return value.flatMap(item => Ok(item + 4))
}
const ok: String \/ String = Ok("Ada")
const short: String \/ String = Ok("x")
const err: String \/ String = Err("original")
const upper = ok.map(value => value.toUpper())
const lengthResult = ok.map(length)
const preservedMap = err.map(failIfCalled)
const directMap = Ok(3).map(value => value * 2)
const flatLength = ok.flatMap(requireLong)
const rejected = short.flatMap(requireLong)
const preservedFlat = err.flatMap(failResultIfCalled)
const directFlat = Ok(5).flatMap(value => Ok(value + 1))
const localMap = mapLocal(Ok(2))
const localFlat = flatMapLocal(Ok(2))
const upperValue = upper ?? "missing"
const lengthValue = lengthResult ?? 0
const directMapValue = directMap ?? 0
const flatLengthValue = flatLength ?? 0
const rejectedError = match rejected { Err(error) => error, _ => "missing" }
const mapError = match preservedMap { Err(error) => error, _ => "missing" }
const flatError = match preservedFlat { Err(error) => error, _ => "missing" }
const directFlatValue = directFlat ?? 0
const localMapValue = localMap ?? 0
const localFlatValue = localFlat ?? 0
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${upperValue}\" \"${lengthValue}\" \"${directMapValue}\" \"${flatLengthValue}\" \"${rejectedError}\" \"${mapError}\" \"${flatError}\" \"${directFlatValue}\" \"${localMapValue}\" \"${localFlatValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("result-map-flat-map.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("case \"$__nacre_result\" in 1*)"));
    assert!(bash.contains("printf '%s' \"$__nacre_result\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ADA|3|6|3|too-short|original|original|6|6|6\n"
    );
}

#[test]
fn generated_bash_runs_result_to_option() {
    let source = r#"
const ok: String? = Ok("ready")?
const err: String? = Err("failed")?
const okValue = ok ?? "missing"
const errValue = err ?? "empty"
const directInt = Ok(9)? ?? 0
try $sh"printf '%s|%s|%s\n' \"${okValue}\" \"${errValue}\" \"${directInt}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("result-to-option.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ready|empty|9\n");
}

#[test]
fn generated_bash_runs_command_option_and_defaults() {
    let source = r#"
const present = $sh"printf ready"? ?? "missing"
const missing = $sh"false"? ?? "empty"
const direct = $sh"false" ?? "fallback"
const piped = ($sh"printf 'a\nb\n'" |> $sh"grep z") ?? "none"
try $sh"printf '%s|%s|%s|%s\n' \"${present}\" \"${missing}\" \"${direct}\" \"${piped}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("command-option-defaults.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("if __nacre_output=\"$(printf ready)\"; then printf '1%s'"));
    assert!(bash.contains("if __nacre_output=\"$(false)\"; then printf '%s'"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ready|empty|fallback|none\n"
    );
}

#[test]
fn generated_bash_matches_command_errors() {
    let source = r#"
const commandCode = match $sh"printf err >&2; exit 7" { Ok(body) => 0 as ExitCode, Err(error) => error.code, _ => 0 as ExitCode }
const commandStderr = match $sh"printf oops >&2; exit 3" { Err({ stderr }) => stderr, _ => "none" }
const commandOk = match $sh"printf ready" { Ok(body) => body, _ => "missing" }
const pipelineCode = match ("alpha\n" |> $sh"grep beta") { Err(error) => error.code, _ => 0 as ExitCode }
const stored: String \/ CmdError = $sh"printf stored >&2; exit 4"
const storedCode = match stored { Err(error) => error.code, _ => 0 as ExitCode }
const storedStderr = match stored { Err({ stderr }) => stderr, _ => "none" }
const storedOk: String \/ CmdError = $sh"printf saved"
const storedOkValue = match storedOk { Ok(value) => value, _ => "missing" }
fn fetchOk(): String \/ CmdError {
return $sh"printf returned"
}
fn fetchErr(): String \/ CmdError {
return $sh"printf returnederr >&2; exit 6"
}
fn fetchPipelineErr(): String \/ CmdError {
return "alpha\n" |> $sh"grep beta"
}
fn implicitFetch(): String \/ CmdError {
$sh"printf implicit"
}
fn implicitPipelineErr(): String \/ CmdError {
"alpha\n" |> $sh"grep beta"
}
const returnOk = fetchOk() ?? "missing"
const returnCode = match fetchErr() { Err(error) => error.code, _ => 0 as ExitCode }
const returnStderr = match fetchErr() { Err({ stderr }) => stderr, _ => "none" }
const returnPipelineCode = match fetchPipelineErr() { Err(error) => error.code, _ => 0 as ExitCode }
const implicitValue = implicitFetch() ?? "missing"
const implicitPipelineCode = match implicitPipelineErr() { Err(error) => error.code, _ => 0 as ExitCode }
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${commandCode}\" \"${commandStderr}\" \"${commandOk}\" \"${pipelineCode}\" \"${storedCode}\" \"${storedStderr}\" \"${storedOkValue}\" \"${returnOk}\" \"${returnCode}\" \"${returnStderr}\" \"${returnPipelineCode}\" \"${implicitValue}\" \"${implicitPipelineCode}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("command-error-match.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_match_stderr_file=\"$(mktemp)\""));
    assert!(bash.contains("readonly stored stored_code stored_stderr"));
    assert!(bash.contains("error_code=\"${__nacre_match_code-}\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "7|oops|ready|1|4|stored|saved|returned|6|returnederr|1|implicit|1\n"
    );
}

#[test]
fn try_command_in_result_function_returns_cmd_error() {
    let source = r#"
const storedTry: String \/ CmdError = try $sh"printf storedtry >&2; exit 5"
const storedTryCode = match storedTry { Err(error) => error.code, _ => 0 as ExitCode }
const storedTryText = match storedTry { Err({ stderr }) => stderr, _ => "none" }
fn guardOk(): String \/ CmdError {
try $sh"printf ignored"
return "done"
}
fn guardReturnOk(): String \/ CmdError {
return try $sh"printf returnedtry"
}
fn guardErr(): String \/ CmdError {
try $sh"printf nope >&2; exit 8"
return "missing"
}
fn guardReturnErr(): String \/ CmdError {
return try $sh"printf returnerr >&2; exit 9"
}
fn guardPipelineErr(): String \/ CmdError {
try ("alpha\n" |> $sh"grep beta")
return "missing"
}
const ok = guardOk() ?? "fallback"
const returnOk = guardReturnOk() ?? "fallback"
const errCode = match guardErr() { Err(error) => error.code, _ => 0 as ExitCode }
const errText = match guardErr() { Err({ stderr }) => stderr, _ => "none" }
const returnErrCode = match guardReturnErr() { Err(error) => error.code, _ => 0 as ExitCode }
const returnErrText = match guardReturnErr() { Err({ stderr }) => stderr, _ => "none" }
const pipelineCode = match guardPipelineErr() { Err(error) => error.code, _ => 0 as ExitCode }
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${storedTryCode}\" \"${storedTryText}\" \"${ok}\" \"${returnOk}\" \"${errCode}\" \"${errText}\" \"${returnErrCode}\" \"${returnErrText}\" \"${pipelineCode}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("try-result-function.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_try_stderr_file=\"$(mktemp)\""));
    assert!(bash.contains("printf '0%s\\037%s'"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "5|storedtry|done|returnedtry|8|nope|9|returnerr|1\n"
    );
}

#[test]
fn try_pipeline_accepts_string_input() {
    let source = r#"
const captured = try ("alpha\nbeta\n" |> $sh"grep beta")
const failed: String \/ CmdError = try ("alpha\n" |> $sh"grep beta")
const failedCode = match failed { Err(error) => error.code, _ => 0 as ExitCode }
try ("alpha\nbeta\n" |> $sh"grep beta")
try $sh"printf '|%s|%s\n' \"${captured}\" \"${failedCode}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("try-string-pipeline.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("printf '%s' 'alpha"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "beta\n|beta|1\n");
}

#[test]
fn try_result_statement_propagates_err_values() {
    let source = r#"
fn okStep(): String \/ String {
return "ok"
}
fn errStep(): String \/ String {
return Err("bad")
}
fn continueAfterOk(): String \/ String {
try okStep()
return "done"
}
fn unwrapOk(): String \/ String {
const value = try okStep()
return "${value}-done"
}
fn stopAfterErr(): String \/ String {
try errStep()
return "missing"
}
fn unwrapErr(): String \/ String {
const value = try errStep()
return "${value}-missing"
}
fn returnTryOk(): String \/ String {
return try okStep()
}
fn returnTryErr(): String \/ String {
return try errStep()
}
const ok = continueAfterOk() ?? "fallback"
const unwrappedOk = unwrapOk() ?? "fallback"
const err = match stopAfterErr() { Err(error) => error, _ => "none" }
const unwrappedErr = match unwrapErr() { Err(error) => error, _ => "none" }
const returnedOk = returnTryOk() ?? "fallback"
const returnedErr = match returnTryErr() { Err(error) => error, _ => "none" }
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${ok}\" \"${unwrappedOk}\" \"${err}\" \"${unwrappedErr}\" \"${returnedOk}\" \"${returnedErr}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("try-result-statement.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "done|ok-done|bad|bad|ok|bad\n"
    );
}

#[test]
fn postfix_bang_propagates_result_and_command_errors() {
    let source = r#"
fn okStep(): String \/ String {
return "ok"
}
fn errStep(): String \/ String {
return Err("bad")
}
fn unwrapOk(): String \/ String {
const value = okStep()!
return "${value}-done"
}
fn stopAfterErr(): String \/ String {
errStep()!
return "missing"
}
fn returnOk(): String \/ String {
return okStep()!
}
fn commandOk(): String \/ CmdError {
const value = $sh"printf command"!
return "${value}-done"
}
fn commandErr(): String \/ CmdError {
$sh"printf command-error >&2; exit 7"!
return "missing"
}
const unwrapped = unwrapOk() ?? "fallback"
const stopped = match stopAfterErr() { Err(error) => error, _ => "none" }
const returned = returnOk() ?? "fallback"
const commandValue = commandOk() ?? "fallback"
const commandCode = match commandErr() { Err(error) => error.code, _ => 0 as ExitCode }
const commandStderr = match commandErr() { Err({ stderr }) => stderr, _ => "none" }
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${unwrapped}\" \"${stopped}\" \"${returned}\" \"${commandValue}\" \"${commandCode}\" \"${commandStderr}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("postfix-bang.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ok-done|bad|ok|command-done|7|command-error\n"
    );
}

#[test]
fn unit_functions_do_not_implicitly_return_tail_values() {
    let source = r#"
fn noop(): Unit {
try $sh"true"
}
fn cleanup(): Unit {
noop()
}
cleanup()
try $sh"printf done"
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("unit-tail-value.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "done");
}

#[test]
fn generated_bash_pipes_string_input_to_command() {
    let source = r#"
const matched = "alpha\nbeta\n" |> $sh"grep beta"
const fallback = ("alpha\n" |> $sh"grep beta") ?? "missing"
try $sh"printf '%s|%s\n' \"${matched}\" \"${fallback}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("string-pipeline.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("printf '%s' "));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "beta|missing\n");
}

#[test]
fn generated_bash_splits_checked_command_output_values() {
    let source = r#"
const parts = try $sh"printf 'one\ntwo words\n'".split("\n")
const braced = $sh{ printf 'braced\nvalue\n'; }.split("\n")
let looped = ""
for part in try $sh"printf 'alpha\nbeta\n'".split("\n") {
looped = "${looped}${part}|"
}
try $sh"printf '%s|%s|%s|%s\n' \"${parts[0]}\" \"${parts[1]}\" \"${braced[1]}\" \"${looped}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("split-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_split_value=\"$(printf 'one"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "one|two words|value|alpha|beta|\n"
    );
}

#[test]
fn generated_bash_trims_command_output_values() {
    let source = r#"
const command = try $sh"printf '  command  '".trim()
const braced = $sh{ printf '  braced  '; }.trim()
const parenthesized = ("  value  ").trim()
try $sh"printf '%s|%s|%s\n' \"${command}\" \"${braced}\" \"${parenthesized}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("trim-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_trim_value=\"$(printf '  command  ')\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "command|braced|value\n"
    );
}

#[test]
fn generated_bash_transforms_command_output_values() {
    let source = r#"
const left = try $sh"printf '  left'".trimStart()
const right = try $sh"printf 'right  '".trimEnd()
const upper = $sh{ printf 'upper'; }.toUpper()
const lower = ("LOWER").toLower()
try $sh"printf '%s|%s|%s|%s\n' \"${left}\" \"${right}\" \"${upper}\" \"${lower}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("transform-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_trim_start_value=\"$(printf '  left')\" || exit $?"));
    assert!(bash.contains("__nacre_trim_end_value=\"$(printf 'right  ')\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "left|right|UPPER|lower\n"
    );
}

#[test]
fn generated_bash_runs_multiline_shell_heredocs() {
    let redirect = temp_path("multiline-heredoc-redirect.txt");
    let source = r#"
fn render(value: String): String {
return $sh"cat <<EOF
local ${value}
EOF"
}
fn propagate(): String \/ CmdError {
const value = $sh"cat <<EOF
propagated
EOF"!
return value
}
const name = "Nacre"
const document = $sh"cat <<EOF
hello ${name}
## preserved shell content
EOF"
const singleQuoted = $sh'cat <<EOF
single quoted
EOF'
const checked = try $sh"cat <<EOF
checked
EOF"
const optional = $sh"cat <<EOF
optional
EOF"?
const optionalValue = optional ?? "missing"
const stored: String \/ CmdError = $sh"cat <<EOF
stored
EOF"
const storedValue = stored ?? "missing"
const matched = match $sh"cat <<EOF
matched
EOF" { Ok(value) => value, _ => "missing" }
const lines = $sh"cat <<EOF
first
second value
EOF".split("\n")
const piped = $sh"cat <<EOF
alpha
beta
EOF" |> $sh"grep beta"
const future = async $sh"cat <<EOF
async
EOF"
const asyncValue = await future
const local = render("scope")
const propagatedValue = propagate() ?? "missing"
$sh"cat <<EOF
redirected
EOF" >> write("__REDIRECT__")
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${document}\" \"${singleQuoted}\" \"${checked}\" \"${optionalValue}\" \"${storedValue}\" \"${matched}\" \"${lines[0]}\" \"${lines[1]}\" \"${piped}\" \"${asyncValue}\" \"${local}\" \"${propagatedValue}\""
"#
    .replace("__REDIRECT__", &redirect.display().to_string());
    let bash = nacre::compile_source(&source).unwrap();
    let script = temp_path("multiline-heredoc.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();
    let redirected = fs::read_to_string(&redirect).unwrap();
    fs::remove_file(&redirect).unwrap();

    assert!(bash.contains("cat <<EOF\nhello ${name}\n## preserved shell content\nEOF"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hello Nacre\n## preserved shell content|single quoted|checked|optional|stored|matched|first|second value|beta|async|local scope|propagated\n"
    );
    assert_eq!(redirected, "redirected\n");
}

#[test]
fn generated_bash_checks_command_output_string_predicates() {
    let source = r#"
const has = try $sh"printf nacre".contains("ac")
const index = try $sh"printf nacre".indexOf("cr")
const starts = $sh{ printf nacre; }.startsWith("na")
const ends = ("nacre").endsWith("re")
try $sh"printf '%s|%s|%s|%s\n' \"${has}\" \"${index}\" \"${starts}\" \"${ends}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("predicate-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_string_value=\"$(printf nacre)\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true|2|true|true\n"
    );
}

#[test]
fn generated_bash_checks_command_output_string_size_methods() {
    let source = r#"
const commandLen = try $sh"printf nacre".len()
const valueLen = ("abcd").len()
const commandEmpty = try $sh"printf ''".isEmpty()
const valueEmpty = ("x").isEmpty()
try $sh"printf '%s|%s|%s|%s\n' \"${commandLen}\" \"${valueLen}\" \"${commandEmpty}\" \"${valueEmpty}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("size-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_string_value=\"$(printf nacre)\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "5|4|true|false\n"
    );
}

#[test]
fn generated_bash_replaces_command_output_values() {
    let source = r#"
const commandReplaced = try $sh"printf 'nacre nacre'".replace("na", "Na")
const valueReplaced = ("space value").replace("space", "center")
const unchanged = ("abc").replace("", "x")
try $sh"printf '%s|%s|%s\n' \"${commandReplaced}\" \"${valueReplaced}\" \"${unchanged}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("replace-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_replace_value=\"$(printf 'nacre nacre')\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Nacre Nacre|center value|abc\n"
    );
}

#[test]
fn generated_bash_slices_command_output_values() {
    let source = r#"
const commandSlice = try $sh"printf nacre".slice(1, 4)
const valueSlice = ("abcd").slice(1, 3)
try $sh"printf '%s|%s\n' \"${commandSlice}\" \"${valueSlice}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("slice-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_string_value=\"$(printf nacre)\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "acr|bc\n");
}

#[test]
fn generated_bash_repeats_command_output_values() {
    let source = r#"
const commandRepeat = try $sh"printf na".repeat(3)
const valueRepeat = ("xo").repeat(2)
try $sh"printf '%s|%s\n' \"${commandRepeat}\" \"${valueRepeat}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("repeat-command-output.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_string_value=\"$(printf na)\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "nanana|xoxo\n");
}

#[test]
fn generated_bash_runs_constructor_match_patterns() {
    let source = r#"
const present: String? = Some("Ada")
const missing: String? = None
const ok: String \/ String = Ok("ready")
const err: String \/ String = Err("failed")
const httpOk: { status: Int, body: String } \/ String = Ok({ status: 200, body: "done" })
const httpErr: String \/ { code: Int } = Err({ code: 7 })
const maybeUser: { name: String }? = Some({ name: "Ada" })
const maybePair: (Int, String)? = Some((200, "tuple"))
const tupleResult: (Int, String) \/ String = Ok((201, "created"))
const presentValue = match present { Some(name) => name, None => "empty", _ => "fallback" }
const missingValue = match missing { Some(name) => name, None => "empty", _ => "fallback" }
const okValue = match ok { Ok("ready") => "literal", Ok(value) => value, Err(error) => error, _ => "fallback" }
const errValue = match err { Ok(value) => value, Err(error) => error, _ => "fallback" }
const httpValue = match httpOk { Ok({ status, body: text }) if status == 200 => text, Ok({ status }) => "other", Err(error) => error, _ => "fallback" }
const errCode = match httpErr { Err({ code }) => code, _ => 0 }
const maybeName = match maybeUser { Some({ name }) => name, _ => "none" }
const tupleValue = match maybePair { Some((code, text)) if code == 200 => text, _ => "none" }
const tupleResultValue = match tupleResult { Ok((201, text)) => text, _ => "none" }
const guarded = match Some(5) { Some(value) if value > 10 => "big", Some(value) if value == 5 => "five", Some(_) => "some", _ => "none" }
const literalGuard = match "a" { "a" if false => "bad", "a" => "ok", _ => "none" }
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${presentValue}\" \"${missingValue}\" \"${okValue}\" \"${errValue}\" \"${httpValue}\" \"${errCode}\" \"${maybeName}\" \"${tupleValue}\" \"${tupleResultValue}\" \"${guarded}\" \"${literalGuard}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("constructor-match-patterns.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Ada|empty|literal|failed|done|7|Ada|tuple|created|five|ok\n"
    );
}

#[test]
fn generated_bash_runs_sum_type_constructors_and_exhaustive_matches() {
    let source = r#"
type LogLevel = Info | Warn | Error
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Label(String)
fn levelName(level: LogLevel): String {
return match level { Info => "info", Warn => "warn", Error => "error" }
}
fn describe(shape: Shape): String {
return match shape {
Circle(radius) if radius > 10.0 => "large circle ${radius}",
Circle(radius) => "circle ${radius}",
Rect(width, height) => "rect ${width}x${height}",
Label(text) => "label ${text}"
}
}
fn makeLabel(value: String): Shape {
return Label(value)
}
const info = levelName(Info)
const warning = levelName(Warn)
const circle = describe(Circle(12.5))
const rect = describe(Rect(3.0, 4.0))
const label = describe(makeLabel("hello world"))
try $sh"printf '%s|%s|%s|%s|%s\n' \"${info}\" \"${warning}\" \"${circle}\" \"${rect}\" \"${label}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("sum-types.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "info|warn|large circle 12.5|rect 3.0x4.0|label hello world\n"
    );
}

#[test]
fn generated_bash_runs_structured_sum_type_payloads() {
    let source = r#"
type Payload =
  | Items([String])
  | Lookup(Map[String, String])
  | User({ name: String, role: String })
  | Pair((String, String))
  | Maybe({ name: String }?)
  | Combined([String], { label: String })
fn describe(value: Payload): String {
return match value {
Items(items) => items.join(","),
Lookup(entries) => entries["key"],
User(user) => match user { { name, role } => name ++ ":" ++ role, _ => "missing" },
Pair(pair) => match pair { (left, right) => left ++ ":" ++ right, _ => "missing" },
Maybe(maybeValue) => match maybeValue { Some({ name }) => name, _ => "missing" },
Combined(values, metadata) => values.join(",") ++ ":" ++ metadata.label
}
}
fn makeUser(): Payload {
const user = { name: "Ada Lovelace", role: "admin" }
return User(user)
}
let items = ["first value", "second"]
const itemPayload = Items(items)
items.push("changed")
let entries: Map[String, String] = { "key": "map value" }
const mapPayload = Lookup(entries)
entries.set("key", "changed")
const arrayText = describe(itemPayload)
const mapText = describe(mapPayload)
const userText = describe(makeUser())
const pairText = describe(Pair(("left side", "7")))
const literalText = describe(Items(["literal"]))
const maybeText = describe(Maybe(Some({ name: "wrapped" })))
const combinedText = describe(Combined(["one", "two"], { label: "group" }))
try $sh"printf '%s|%s|%s|%s|%s|%s|%s\n' \"${arrayText}\" \"${mapText}\" \"${userText}\" \"${pairText}\" \"${literalText}\" \"${maybeText}\" \"${combinedText}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("structured-sum-types.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "first value,second|map value|Ada Lovelace:admin|left side:7|literal|wrapped|one,two:group\n"
    );
}

#[test]
fn generated_bash_runs_structured_sum_types_without_functions() {
    let source = r#"
type Boxed = Box([String]) | Empty
const value: Boxed = Box(["top level", "value"])
const output = match value {
Box(items) => items.join(":"),
Empty => "empty"
}
try $sh"printf '%s\n' \"${output}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("structured-sum-types-top-level.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "top level:value\n",
        "stderr:\n{}\nbash:\n{bash}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generated_bash_runs_exhaustive_builtin_matches() {
    let source = r#"
fn optionLabel(value: String?): String {
return match value {
Some(text) if text.isEmpty() => "empty",
Some(text) => text,
None => "none"
}
}
fn resultLabel(value: String \/ Int): String {
return match value {
Ok(text) => text,
Err(code) => "error ${code}"
}
}
fn boolLabel(value: Bool): String {
return match value { true => "yes", false => "no" }
}
const present = optionLabel(Some("ready"))
const missing = optionLabel(None)
const ok = resultLabel(Ok("done"))
const err = resultLabel(Err(7))
const yes = boolLabel(true)
const no = boolLabel(false)
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${present}\" \"${missing}\" \"${ok}\" \"${err}\" \"${yes}\" \"${no}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("exhaustive-builtins.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ready|none|done|error 7|yes|no\n"
    );
}

#[test]
fn compile_file_namespaces_sum_types_and_variants() {
    let root = temp_path("sum-type-module");
    fs::create_dir_all(&root).unwrap();
    let module = root.join("shapes.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
type Shape = Circle(Float) | Rect(Float, Float)
fn describe(shape: Shape): String {
return match shape {
Circle(radius) => "circle ${radius}",
Rect(width, height) => "rect ${width}x${height}"
}
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use shapes
const shape: shapes.Shape = shapes.Rect(2.0, 5.0)
const label = shapes.describe(shape)
try $sh"printf '%s\n' \"${label}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("sum-type-module.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "rect 2.0x5.0\n");
}

#[test]
fn generated_bash_runs_newtype_constructor_override() {
    let source = r#"
newtype UserId = Int
newtype PlainId = Int
fn! UserId(value: Int): UserId \/ String {
if value < 0 {
return Err("negative")
}
return value as UserId
}
const ok = UserId(7) ?? (0 as UserId)
const err = match UserId(-1) { Err(error) => error, _ => "none" }
const direct = PlainId(3)
try $sh"printf '%s|%s|%s\n' \"${ok}\" \"${err}\" \"${direct}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("newtype-constructor-override.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("UserId() {"));
    assert!(bash.contains("readonly ok=$(__nacre_option=\"$(__nacre_call \"$UserId\" 7)\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "7|negative|3\n");
}

#[test]
fn generated_bash_runs_tuple_match_patterns() {
    let source = r#"
const code = 200
const method = "GET"
const pair = (code, method)
const status = match (code, method) { (200, "POST") => "wrong", (200, "GET") => "ok", (_, "DELETE") => "delete", _ => "other" }
const guarded = match (500, "GET") { (500, "GET") if false => "bad", (500, _) => "server", _ => "other" }
const bound = match (201, "POST") { (200, name) => name, (201, name) if name == "POST" => name, _ => "none" }
const variablePair = match pair { (200, name) => name, _ => "none" }
try $sh"printf '%s|%s|%s|%s\n' \"${status}\" \"${guarded}\" \"${bound}\" \"${variablePair}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("tuple-match-patterns.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ok|server|POST|GET\n"
    );
}

#[test]
fn generated_bash_runs_record_match_patterns() {
    let source = r#"
const user = { name: "Ada", role: "admin" }
const literal = match user { { name: "Ada", role } if role == "admin" => role, _ => "none" }
const bound = match user { { name } => name, _ => "unknown" }
const inline = match { name: "Grace", role: "user" } { { name, role: "user" } => name, _ => "unknown" }
try $sh"printf '%s|%s|%s\n' \"${literal}\" \"${bound}\" \"${inline}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("record-match-patterns.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "admin|Ada|Grace\n"
    );
}

#[test]
fn generated_bash_runs_spawn_and_wait_method() {
    let source = r#"
const first = spawn $sh"printf first"
const second = async $sh"printf second"
const firstOut = first.wait()
const secondOut = await second
try $sh"printf '%s|%s\n' \"${firstOut}\" \"${secondOut}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("spawn-wait.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("first_out=\"$(mktemp)\""));
    assert!(bash.contains("printf first > \"$first_out\" 2>&1 &"));
    assert!(bash.contains("if wait \"$first_pid\"; then"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "first|second\n");
}

#[test]
fn generated_bash_exposes_script_args_as_string_array() {
    let source = r#"
const first = args[0]
const count = args.len()
const [command, ...rest] = args
fn joinArgs(args: ...String): String {
return args.join(":")
}
const joined = joinArgs("fn", "param")
try $sh"echo ${first} ${count} ${command} ${rest[0]} ${joined}"
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("script-args.sh");
    fs::write(&script, bash).unwrap();
    let output = Command::new("bash")
        .arg(&script)
        .arg("run")
        .arg("target")
        .output()
        .unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "run 2 run target fn:param\n"
    );
}

#[test]
fn generated_bash_pushes_and_pops_array_items() {
    let source = r#"
let names = ["alice"]
names.push("bob")
names.push("carol")
names.pop()
let paths: [Path] = ["/tmp"]
paths.push("/var")
paths.pop()
const joined = names.join(",")
const pathList = paths.join(":")
const firstName = names.first()
const lastName = names.last()
const firstPath = paths.first()
const lastPath = paths.last()
const reversedNames = names.reverse()
const reversedPaths = paths.reverse()
const reversedJoined = reversedNames.join(",")
const reversedPathList = reversedPaths.join(":")
const hasBob = names.contains("bob")
const hasCarol = names.contains("carol")
const hasTmp = paths.contains("/tmp")
const bobIndex = names.indexOf("bob")
const carolIndex = names.indexOf("carol")
const tmpIndex = paths.indexOf("/tmp")
try $sh"echo ${joined} ${pathList} ${firstName} ${lastName} ${firstPath} ${lastPath} ${reversedJoined} ${reversedPathList} ${hasBob} ${hasCarol} ${hasTmp} ${bobIndex} ${carolIndex} ${tmpIndex}"
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("array-push.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("names+=('bob')"));
    assert!(bash.contains("names+=('carol')"));
    assert!(bash.contains("paths+=('/var')"));
    assert!(bash.contains("readonly firstName=\"${names[0]}\""));
    assert!(bash.contains("readonly lastName=$(if [ \"${#names[@]}\" -gt 0 ]; then printf '%s' \"${names[$((${#names[@]} - 1))]}\"; fi)"));
    assert!(bash.contains("reversedNames=()"));
    assert!(bash.contains("reversedNames+=(\"${names[$__nacre_i]}\")"));
    assert!(bash.contains("readonly -a reversedNames"));
    assert!(bash.contains("for __nacre_item in \"${names[@]}\""));
    assert!(bash.contains("for __nacre_item in \"${paths[@]}\""));
    assert!(bash.contains("__nacre_index=-1; __nacre_i=0"));
    assert!(bash.contains("unset \"names[$((${#names[@]} - 1))]\""));
    assert!(bash.contains("unset \"paths[$((${#paths[@]} - 1))]\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "alice,bob /tmp alice bob /tmp /tmp bob,alice /tmp true false true 1 -1 0\n"
    );
}

#[test]
fn generated_bash_sets_and_removes_map_entries() {
    let source = r#"
fn updateLocal(): String {
let local: Map[String, String] = {}
local.set("key", "local value")
local.set("unused", "remove me")
local.remove("unused")
return local["key"]
}
let envs: Map[String, String] = {}
const portKey = "PORT"
envs.set(portKey, "8080")
envs.set(portKey, "9090")
envs.set("SPACE KEY", "enabled value")
envs.set("OLD", "remove me")
envs.remove("OLD")
envs.remove("MISSING")
const port = envs["PORT"]
const spaced = envs["SPACE KEY"]
const hasOld = envs.has("OLD")
const hasSpace = envs.has("SPACE KEY")
const count = envs.len()
const localValue = updateLocal()
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${port}\" \"${spaced}\" \"${hasOld}\" \"${hasSpace}\" \"${count}\" \"${localValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("map-set-remove.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "9090|enabled value|false|true|2|local value\n"
    );
}

#[test]
fn nested_result_propagation_short_circuits_eager_expressions() {
    let source = r#"
fn textOk(): String \/ String {
return "ok"
}
fn textErr(): String \/ String {
return Err("bad")
}
fn intOk(): Int \/ String {
return 4
}
fn boolOk(): Bool \/ String {
return true
}
fn boolErr(): Bool \/ String {
return Err("condition")
}
fn decorate(prefix: String, value: String): String {
return "${prefix}${value}"
}
fn nestedOk(): String \/ String {
const __nacre_try_value_0 = "reserved"
const called = decorate("value:", textOk()!)
const viaTry = decorate("try:", try textOk())
const sum = intOk()! + 2
const optional = Some(textOk()!)
const optionalValue = optional ?? "missing"
let assigned = ""
assigned = decorate("assigned:", textOk()!)
return "${__nacre_try_value_0}|${called}|${viaTry}|${sum}|${optionalValue}|${assigned}"
}
fn nestedErr(): String \/ String {
return decorate("never:", textErr()!)
}
fn nestedBranch(): String \/ String {
if boolOk()! {
return decorate("branch:", textOk()!)
}
return "missing"
}
fn nestedConditionErr(): String \/ String {
if boolErr()! {
return "missing"
}
return "also missing"
}
fn nestedCommandOk(): String \/ CmdError {
return decorate("cmd:", $sh"printf shell"!)
}
fn nestedCommandErr(): String \/ CmdError {
return decorate("never:", $sh"printf command-error >&2; exit 7"!)
}
const ok = nestedOk() ?? "fallback"
const err = match nestedErr() { Err(value) => value, _ => "missing" }
const branch = nestedBranch() ?? "fallback"
const conditionErr = match nestedConditionErr() { Err(value) => value, _ => "missing" }
const command = nestedCommandOk() ?? "fallback"
const commandCode = match nestedCommandErr() { Err(value) => value.code, _ => 0 as ExitCode }
const commandText = match nestedCommandErr() { Err({ stderr }) => stderr, _ => "missing" }
try $sh"printf '%s|%s|%s|%s|%s|%s|%s\n' \"${ok}\" \"${err}\" \"${branch}\" \"${conditionErr}\" \"${command}\" \"${commandCode}\" \"${commandText}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("nested-result-propagation.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_try_value_1"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "reserved|value:ok|try:ok|6|ok|assigned:ok|bad|branch:ok|condition|cmd:shell|7|command-error\n"
    );
}

#[test]
fn nested_result_propagation_preserves_lazy_branches() {
    let source = r#"
fn boolOk(): Bool \/ String {
return true
}
fn boolErr(): Bool \/ String {
return Err("bool-error")
}
fn textOk(): String \/ String {
return "ok"
}
fn textErr(): String \/ String {
return Err("text-error")
}
fn optionOk(): String? \/ String {
return Some("option")
}
fn optionErr(): String? \/ String {
return Err("option-error")
}
fn guardOk(): Bool \/ String {
return true
}
fn guardFalse(): Bool \/ String {
return false
}
fn guardErr(): Bool \/ String {
return Err("guard-error")
}
fn decorate(prefix: String, value: String): String {
return "${prefix}${value}"
}
fn logicalSkipped(): Bool \/ String {
const leftFalse = false && boolErr()!
const leftTrue = true || boolErr()!
const andValue = true && boolOk()!
const orValue = false || boolOk()!
const bothPropagated = boolOk()! && boolOk()!
return leftFalse == false && leftTrue && andValue && orValue && bothPropagated
}
fn logicalError(): Bool \/ String {
return true && boolErr()!
}
fn choose(flag: Bool): String \/ String {
return if flag { decorate("if:", textOk()!) } else { decorate("else:", textErr()!) }
}
fn pick(value: String): String \/ String {
return match value {
"ok" => decorate("match:", textOk()!),
_ => decorate("other:", textErr()!)
}
}
fn pickPayload(input: String?): String \/ String {
return match input {
Some(item) => decorate("${item}:", textOk()!),
None => "none"
}
}
fn guarded(value: String): String \/ String {
return match value {
"hit" if guardOk()! => "guarded",
"hit" => "fallback",
_ => "other"
}
}
fn guardedFalse(value: String): String \/ String {
return match value {
"hit" if guardFalse()! => "bad",
"hit" => "guard-fallback",
_ => "other"
}
}
fn guardedError(value: String): String \/ String {
return match value {
"hit" if guardErr()! => "bad",
"hit" => "must-not-run",
_ => "other"
}
}
fn guardNotSelected(): String \/ String {
return match "other" {
"hit" if guardErr()! => "bad",
_ => "not-selected"
}
}
fn guardedPayload(input: String?): String \/ String {
return match input {
Some(item) if guardOk()! => "${item}:guard",
_ => "none"
}
}
fn defaultSkipped(): String \/ String {
const present: String? = Some("present")
return present ?? textErr()!
}
fn defaultUsed(): String \/ String {
const missing: String? = None
return missing ?? textOk()!
}
fn defaultError(): String \/ String {
const missing: String? = None
return missing ?? textErr()!
}
fn alternativeSkipped(): String? \/ String {
const present: String? = Some("kept")
return present <|> optionErr()!
}
fn alternativeUsed(): String? \/ String {
const missing: String? = None
return missing <|> optionOk()!
}
fn alternativeError(): String? \/ String {
const missing: String? = None
return missing <|> optionErr()!
}
fn commandFallback(): String \/ CmdError {
return $sh"false" ?? $sh"printf command-fallback"!
}
const logical = logicalSkipped() ?? false
const logicalErr = match logicalError() { Err(value) => value, _ => "missing" }
const chosen = choose(true) ?? "missing"
const chooseErr = match choose(false) { Err(value) => value, _ => "missing" }
const selected = pick("ok") ?? "missing"
const selectErr = match pick("other") { Err(value) => value, _ => "missing" }
const payload = pickPayload(Some("payload")) ?? "missing"
const guardValue = guarded("hit") ?? "missing"
const guardFallback = guardedFalse("hit") ?? "missing"
const guardError = match guardedError("hit") { Err(value) => value, _ => "missing" }
const guardSkipped = guardNotSelected() ?? "missing"
const guardPayload = guardedPayload(Some("payload")) ?? "missing"
const defaultKept = defaultSkipped() ?? "missing"
const defaultValue = defaultUsed() ?? "missing"
const defaultErr = match defaultError() { Err(value) => value, _ => "missing" }
const alternativeKept = (alternativeSkipped() ?? None) ?? "missing"
const alternativeValue = (alternativeUsed() ?? None) ?? "missing"
const alternativeErr = match alternativeError() { Err(value) => value, _ => "missing" }
const commandValue = commandFallback() ?? "missing"
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${logical}\" \"${logicalErr}\" \"${chosen}\" \"${chooseErr}\" \"${selected}\" \"${selectErr}\" \"${payload}\" \"${guardValue}\" \"${guardFallback}\" \"${guardError}\" \"${guardSkipped}\" \"${guardPayload}\" \"${defaultKept}\" \"${defaultValue}\" \"${defaultErr}\" \"${alternativeKept}\" \"${alternativeValue}\" \"${alternativeErr}\" \"${commandValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("nested-lazy-result-propagation.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true|bool-error|if:ok|text-error|match:ok|text-error|payload:ok|guarded|guard-fallback|guard-error|not-selected|payload:guard|present|ok|text-error|kept|option|option-error|command-fallback\n"
    );
}

#[test]
fn generated_bash_mangles_reserved_function_names() {
    let source = r#"
fn select(value: String): String {
return "${value}:selected"
}
fn apply(f: String => String, value: String): String {
return f(value)
}
const direct = select("direct")
const indirect = apply(select, "indirect")
try $sh"printf '%s|%s\n' \"${direct}\" \"${indirect}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("reserved-function-name.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_keyword_select() {"));
    assert!(bash.contains("readonly select='__nacre_keyword_select'"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "direct:selected|indirect:selected\n"
    );
}

#[test]
fn generated_bash_runs_non_capturing_lambdas() {
    let source = r#"
fn applyInt(f: Int => Int, value: Int): Int {
return f(value)
}
fn combine(f: (String, String) => String, left: String, right: String): String {
return f(left, right)
}
fn applyTo(value: Int, f: Int => Int): Int {
return f(value)
}
fn applyLocal(value: Int): Int {
const local: Int => Int = item => item + 4
return local(value)
}
const double: Int => Int = value => value * 2
const doubled = double(6)
const incremented = applyInt(value => value + 1, 6)
const joined = combine((left, right) => left ++ ":" ++ right, "a", "b")
const base = 8
const viaMethod = base.applyTo(value => value + 2)
const viaLocal = applyLocal(6)
let transform: Int => Int = value => value - 1
transform = value => value * 3
const transformed = transform(5)
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${doubled}\" \"${incremented}\" \"${joined}\" \"${viaMethod}\" \"${viaLocal}\" \"${transformed}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("lambdas.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(bash.contains("__nacre_lambda_0() {"));
    assert!(bash.contains("__nacre_lambda_1() {"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "12|7|a:b|10|10|15\n"
    );
}

#[test]
fn generated_bash_runs_capturing_lambdas_after_their_scope_ends() {
    let source = r#"
fn applyInt(f: Int => Int, value: Int): Int {
return f(value)
}
fn makeAdder(amount: Int): Int => Int {
const offset = amount + 1
return value => value + offset
}
fn makeNested(prefix: String): String => String => String {
return left => right => prefix ++ ":" ++ left ++ ":" ++ right
}
fn after(f: Int => Int): Int => Int {
return value => f(value) + 1
}
const base = 10
const addBase: Int => Int = value => value + base
const direct = addBase(2)
const throughFunction = applyInt(value => value + base, 3)
let snapshot = 1
const addSnapshot: Int => Int = value => value + snapshot
snapshot = 9
const snapshotValue = addSnapshot(1)
const decoratedPrefix = "a:b "
const decorate: String => String = value => decoratedPrefix ++ value
const decorated = decorate("ok")
const addSix = makeAdder(5)
const escaped = addSix(4)
const addSeven = after(addSix)
const composed = addSeven(4)
const nested = makeNested("root")
const withLeft = nested("left")
const nestedValue = withLeft("right")
const values = [1, 2]
const mapped = values.map(value => value + base)
const option: Int? = Some(5)
const mappedOption = option.map(value => value + base)
const mappedOptionValue = mappedOption ?? 0
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${direct}\" \"${throughFunction}\" \"${snapshotValue}\" \"${decorated}\" \"${escaped}\" \"${composed}\" \"${nestedValue}\" \"${mapped[0]}\" \"${mapped[1]}\" \"${mappedOptionValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("capturing-lambdas.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(bash.contains("__nacre_closure_pack"));
    assert!(bash.contains("__nacre_call"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "12|13|2|a:b ok|10|11|root:left:right|11|12|15\n",
        "bash:\n{bash}"
    );
}

#[test]
fn generated_bash_snapshots_structured_closure_captures() {
    let source = r#"
type Reader = () => String
type NestedReader = () => Reader
fn makeReader(): Reader {
let values = ["array value", "second"]
let labels: Map[String, String] = { "key": "map value" }
const user = { name: "Ada", role: "admin" }
const pair = ("left", "right")
const maybe: { name: String }? = Some({ name: "wrapped" })
const outcome: (String, String) \/ String = Ok(("result", "tuple"))
const reader: Reader = () => values[0] ++ "|" ++ labels["key"] ++ "|" ++ user.name ++ ":" ++ user.role ++ "|" ++ pair._1 ++ ":" ++ pair._2 ++ "|" ++ (match maybe { Some({ name }) => name, _ => "missing" }) ++ "|" ++ (match outcome { Ok((left, right)) => left ++ ":" ++ right, _ => "missing" })
values.push("after")
labels.set("key", "changed")
return reader
}
fn makeNestedReader(): NestedReader {
const values = ["nested value"]
return () => () => values[0]
}
const reader = makeReader()
const output = reader()
const outer = makeNestedReader()
const inner = outer()
const nested = inner()
try $sh"printf '%s|%s\n' \"${output}\" \"${nested}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("structured-closure-captures.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "array value|map value|Ada:admin|left:right|wrapped|result:tuple|nested value\n",
        "bash:\n{bash}"
    );
}

#[test]
fn generated_bash_propagates_results_inside_lambda_bodies() {
    let source = r#"
fn step(value: Int): Int \/ String {
if value < 0 {
return Err("negative")
}
return value + 1
}
fn boolError(): Bool \/ String {
return Err("bool-error")
}
fn applyResult(f: Int => Int \/ String, value: Int): Int \/ String {
return f(value)
}
const offset = 10
const transform: Int => Int \/ String = value => step(value)! + offset
const transformTry: Int => Int \/ String = value => try step(value)
const lazyAnd: Bool => Bool \/ String = value => value && boolError()!
const command: Bool => String \/ CmdError = fail => if fail { $sh"printf command-error >&2; exit 9"! } else { $sh"printf command-ok"! }
const values = [2, -1]
const mapped = values.map(value => step(value)!)
const ok = transform(2) ?? 0
const tryOk = transformTry(4) ?? 0
const err = match transform(-1) { Err(value) => value, _ => "missing" }
const passed = applyResult(value => step(value)! * 2, 3) ?? 0
const skipped = lazyAnd(false) ?? true
const lazyErr = match lazyAnd(true) { Err(value) => value, _ => "missing" }
const commandOk = command(false) ?? "missing"
const commandCode = match command(true) { Err(value) => value.code, _ => 0 as ExitCode }
const mappedOk = mapped[0] ?? 0
const mappedErr = match mapped[1] { Err(value) => value, _ => "missing" }
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${ok}\" \"${tryOk}\" \"${err}\" \"${passed}\" \"${skipped}\" \"${lazyErr}\" \"${commandOk}\" \"${commandCode}\" \"${mappedOk}\" \"${mappedErr}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("lambda-result-propagation.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "13|5|negative|8|false|bool-error|command-ok|9|3|negative\n"
    );
}

#[test]
fn generated_bash_runs_option_and_result_do_expressions() {
    let source = r#"
fn failOption(): Int? {
$sh"false"
return Some(9)
}
fn failResult(): Int \/ String {
$sh"false"
return Ok(9)
}
fn optionSum(first: Int?, second: Int?): Int? {
return do {
left <- first
const offset: Int = left + 1
right <- second
pure(offset + right)
}
}
fn resultSum(first: Int \/ String, second: Int \/ String): Int \/ String {
return do {
left <- first
let offset = left + 2
right <- second
pure(offset + right)
}
}
const optionOk = optionSum(Some(2), Some(3))
const optionNone = optionSum(None, failOption())
const resultOk = resultSum(Ok(4), Ok(5))
const resultFirstError = resultSum(Err("first"), failResult())
const resultSecondError = resultSum(Ok(1), Err("second"))
const directTail = do {
value <- Some(3)
Some(value * 2)
}
const optionOkValue = optionOk ?? 0
const optionNoneValue = optionNone ?? 7
const resultOkValue = resultOk ?? 0
const firstError = match resultFirstError { Err(error) => error, _ => "missing" }
const secondError = match resultSecondError { Err(error) => error, _ => "missing" }
const directTailValue = directTail ?? 0
try $sh"printf '%s|%s|%s|%s|%s|%s\n' \"${optionOkValue}\" \"${optionNoneValue}\" \"${resultOkValue}\" \"${firstError}\" \"${secondError}\" \"${directTailValue}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("do-expressions.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "6|7|11|first|second|6\n"
    );
}

#[test]
fn generated_bash_runs_structured_do_locals() {
    let source = r#"
fn buildOption(seed: String?): String? {
return do {
value <- seed
const items = [value, "two words"]
const copiedItems = items
const labels = { "primary": copiedItems.join(",") }
const copiedLabels = labels
const user = { name: copiedLabels["primary"], role: copiedItems[1] }
const copiedUser = user
const pair = (copiedUser.name, copiedUser.role)
const copiedPair = pair
const maybeUser: { name: String }? = Some({ name: copiedPair._1 })
const wrappedName = match maybeUser { Some({ name }) => name, None => "missing" }
const rolesText = copiedItems.join("+")
pure(wrappedName ++ ":" ++ rolesText)
}
}
fn buildResult(seed: String \/ String): String \/ String {
return do {
value <- seed
const pair = (value, "result")
const wrapped: (String, String) \/ String = Ok((pair._1, pair._2))
const text = match wrapped { Ok((left, right)) => left ++ ":" ++ right, Err(error) => error }
pure(text)
}
}
const direct = do {
const words = ["top level", "value"]
const copied = words
Some(copied.join(":"))
}
const optionText = buildOption(Some("first")) ?? "none"
const resultText = buildResult(Ok("ok")) ?? "error"
const directText = direct ?? "none"
try $sh"printf '%s|%s|%s\n' \"${optionText}\" \"${resultText}\" \"${directText}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("structured-do-locals.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "first,two words:first+two words|ok:result|top level:value\n",
        "stderr:\n{}\nbash:\n{bash}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generated_bash_maps_arrays_with_lambdas_and_functions() {
    let source = r#"
fn length(value: String): Int {
return value.len()
}
fn mapLocal(value: Int): Int {
const values = [value, value + 2]
const mapped = values.map(item => item + 10)
return mapped[1]
}
const numbers = [1, 2, 3]
const doubled = numbers.map(value => value * 2)
const literal = ([4, 5]).map(value => value + 1)
const names = ["a b", "cd"]
const upper = names.map(value => value.toUpper())
const lengths = names.map(length)
const local = mapLocal(1)
let looped = ""
for value in numbers.map(value => value * 3) {
looped = looped ++ "${value}|"
}
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${doubled[0]}\" \"${doubled[2]}\" \"${literal[0]}\" \"${literal[1]}\" \"${upper[0]}\" \"${upper[1]}\" \"${lengths[0]}\" \"${lengths[1]}\" \"${local}\" \"${looped}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("array-map.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_lambda_0() {"));
    assert!(bash.contains("for __nacre_item in \"${numbers[@]}\""));
    assert!(bash.contains("for value in \"${__nacre_array_map_iter[@]}\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "2|6|5|6|A B|CD|3|2|13|3|6|9|\n"
    );
}

#[test]
fn generated_bash_iterates_reversed_arrays_without_word_splitting() {
    let source = r#"
const names = ["alice alpha", "bob"]
let looped = ""
for name in names.reverse() {
looped = looped ++ name ++ "|"
}
try $sh"test \"${looped}\" = \"bob|alice alpha|\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("array-reverse-loop.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_reverse_iter=()"));
    assert!(bash.contains("for name in \"${__nacre_reverse_iter[@]}\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
}

#[test]
fn generated_bash_sorts_arrays_and_preserves_words() {
    let source = r#"
const names = ["bob", "alice alpha", "carol"]
const sorted = names.sort()
const sortedJoined = sorted.join("|")
let looped = ""
for name in names.sort() {
looped = looped ++ name ++ "|"
}
try $sh"test \"${sortedJoined}\" = \"alice alpha|bob|carol\""
try $sh"test \"${looped}\" = \"alice alpha|bob|carol|\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("array-sort.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("mapfile -t sorted < <(printf '%s\\n' \"${names[@]}\" | sort)"));
    assert!(bash.contains("__nacre_sort_iter=()"));
    assert!(bash.contains("for name in \"${__nacre_sort_iter[@]}\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
}

#[test]
fn generated_bash_uniques_arrays_preserving_first_seen_order() {
    let source = r#"
const names = ["", "bob", "alice", "bob", "", "alice"]
const unique = names.unique()
const uniqueJoined = unique.join("|")
let looped = ""
for name in names.unique() {
looped = looped ++ "[" ++ name ++ "]"
}
try $sh"test \"${uniqueJoined}\" = \"|bob|alice\""
try $sh"test \"${looped}\" = \"[][bob][alice]\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("array-unique.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("unique=()"));
    assert!(bash.contains("unique+=(\"$__nacre_item\")"));
    assert!(bash.contains("__nacre_unique_iter=()"));
    assert!(bash.contains("for name in \"${__nacre_unique_iter[@]}\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
}

#[test]
fn generated_bash_takes_and_drops_arrays() {
    let source = r#"
const names = ["alice alpha", "bob", "carol"]
const firstTwo = names.take(2)
const afterFirst = names.drop(1)
const firstTwoJoined = firstTwo.join("|")
const afterFirstJoined = afterFirst.join("|")
let looped = ""
for name in names.drop(1) {
looped = looped ++ name ++ "|"
}
try $sh"test \"${firstTwoJoined}\" = \"alice alpha|bob\""
try $sh"test \"${afterFirstJoined}\" = \"bob|carol\""
try $sh"test \"${looped}\" = \"bob|carol|\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("array-take-drop.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("readonly -a firstTwo=(\"${names[@]:$((0)):$((2 - 0))}\")"));
    assert!(bash.contains("readonly -a afterFirst=(\"${names[@]:$((1))}\")"));
    assert!(bash.contains("for name in \"${names[@]:$((1))}\"; do"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
}

#[test]
fn generated_function_body_runs_shell_pipeline_and_redirects() {
    let redirect = temp_path("function-redirect.txt");
    let source = r#"
fn runShell(value: String): String {
$sh"printf side"
const piped = $sh"printf ${value}" |> $sh"tr a-z A-Z"
$sh"printf write" >> write("__REDIRECT__")
$sh"printf append" >> append("__REDIRECT__")
return piped
}
const out = runShell("pipe")
try $sh'echo "${out}"'
"#
    .replace("__REDIRECT__", &redirect.display().to_string());
    let bash = nacre::compile_source(&source).unwrap();
    let script = temp_path("function-shell.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "sidePIPE\n");
    assert_eq!(fs::read_to_string(&redirect).unwrap(), "writeappend");
    fs::remove_file(redirect).unwrap();
}

#[test]
fn generated_redirect_can_capture_stderr() {
    let root = temp_path("redirect-stderr");
    fs::create_dir_all(&root).unwrap();
    let out_path = root.join("out.txt");
    let err_path = root.join("err.txt");
    let source = r#"
const dir = "__DIR__"
$sh"sh -c 'printf out; printf err >&2'" >> write("${dir}/out.txt", stderr = "${dir}/err.txt")
$sh"sh -c 'printf more; printf moreerr >&2'" >> append("${dir}/out.txt", stderr = "${dir}/err.txt")
"#
    .replace("__DIR__", &root.display().to_string());

    let bash = nacre::compile_source(&source).unwrap();
    let script = root.join("redirect-stderr.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(out_path).unwrap(), "outmore");
    assert_eq!(fs::read_to_string(err_path).unwrap(), "errmoreerr");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn compile_file_namespaces_impl_method_bodies() {
    let root = temp_path("module-impl");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    let module = lib.join("utils.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
trait Show[T] {
fn show(value: T): String
}
fn wrap(value: String): String {
return "wrapped ${value}"
}
impl Show[String] {
fn show(value: String): String {
return wrap(value)
}
}
fn moduleShown(value: String): String {
return Show.show(value)
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.utils
trait Show[T] {
fn show(value: T): String
}
impl Show[String] {
fn show(value: String): String {
return "local ${value}"
}
}
const imported = utils.Show.show("module")
const local = Show.show("module")
const bounded = utils.moduleShown("module")
try $sh'echo "${imported}|${local}|${bounded}"'
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("module-impl.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("__nacre_trait_utils_Show_String_show"));
    assert!(bash.contains("__nacre_trait_Show_String_show"));
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "wrapped module|local module|wrapped module\n"
    );
}

#[test]
fn compile_file_resolves_index_modules_and_missing_modules() {
    let root = temp_path("index-module");
    let module_dir = root.join("lib").join("utils");
    fs::create_dir_all(&module_dir).unwrap();
    let module = module_dir.join("index.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
fn label(value: String): String {
return "index ${value}"
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.utils
const message = utils.label("module")
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    assert!(bash.contains("utils.label() {"));
    assert!(bash.contains("readonly message=\"$(utils.label 'module')\""));

    fs::write(&main, "use lib.missing\n").unwrap();
    let error = nacre::compile_file(&main).unwrap_err();
    fs::remove_dir_all(&root).unwrap();

    assert!(error
        .message()
        .contains("module `lib.missing` was not found"));
}

#[test]
fn compile_file_resolves_std_path_module() {
    let root = temp_path("std-path");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.path
const joined = path.join("tmp", "nacre", "file.txt")
const absolute = path.isAbsolute("/tmp/nacre")
const relative = path.isAbsolute(joined)
const base = path.basename(joined)
const dir = path.dirname(joined)
const stem = path.stem(joined)
const ext = path.extname(joined)
const noExt = path.extname("README")
const dotExt = path.extname(".env")
const nestedDotExt = path.extname(".config.json")
const noExtStem = path.stem("README")
const dotStem = path.stem(".env")
const nestedDotStem = path.stem(".config.json")
try $sh"echo ${joined}"
try $sh"printf '%s\n' \"${absolute}|${relative}|${base}|${dir}|${stem}|${ext}|${noExt}|${dotExt}|${nestedDotExt}|${noExtStem}|${dotStem}|${nestedDotStem}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-path.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("path.join() {"));
    assert!(bash.contains("path.basename() {"));
    assert!(bash.contains("path.dirname() {"));
    assert!(bash.contains("path.stem() {"));
    assert!(bash.contains("path.extname() {"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "tmp/nacre/file.txt\ntrue|false|file.txt|tmp/nacre|file|.txt|||.json|README|.env|.config\n"
    );
}

#[test]
fn generated_bash_runs_path_methods() {
    let root = temp_path("path-methods");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
fn inspect(value: Path): String {
const base = value.basename()
const dir = value.dirname()
const stem = value.stem()
const ext = value.extname()
return "${base}|${dir}|${stem}|${ext}"
}
const file: Path = "/tmp/nacre/file.txt"
const nested = ".config.json"
const hidden = ".env"
const plain = "README"
const relative: Path = "tmp/nacre"
const summary = inspect(file)
const fileAbsolute = file.isAbsolute()
const relativeAbsolute = relative.isAbsolute()
const nestedStem = nested.stem()
const nestedExt = nested.extname()
const hiddenStem = hidden.stem()
const hiddenExt = hidden.extname()
const plainStem = plain.stem()
const plainExt = plain.extname()
try $sh"printf '%s\n' \"${summary}|${fileAbsolute}|${relativeAbsolute}|${nestedStem}|${nestedExt}|${hiddenStem}|${hiddenExt}|${plainStem}|${plainExt}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("path-methods.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("basename \"$"));
    assert!(bash.contains("dirname \"$"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "file.txt|/tmp/nacre|file|.txt|true|false|.config|.json|.env||README|\n"
    );
}

#[test]
fn generated_bash_runs_path_methods_on_values_and_command_output() {
    let source = r#"
const valueBase = ("/tmp/nacre/file.txt").basename()
const valueDir = ("/tmp/nacre/file.txt").dirname()
const valueStem = (".config.json").stem()
const valueExt = (".config.json").extname()
const valueAbsolute = ("/tmp/nacre").isAbsolute()
const commandBase = try $sh"printf /tmp/nacre/file.txt".basename()
const commandDir = try $sh"printf /tmp/nacre/file.txt".dirname()
const commandStem = try $sh"printf /tmp/nacre/file.txt".stem()
const commandExt = try $sh"printf /tmp/nacre/file.txt".extname()
const commandAbsolute = try $sh"printf /tmp/nacre".isAbsolute()
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \"${valueBase}\" \"${valueDir}\" \"${valueStem}\" \"${valueExt}\" \"${valueAbsolute}\" \"${commandBase}\" \"${commandDir}\" \"${commandStem}\" \"${commandExt}\" \"${commandAbsolute}\""
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("path-values.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("__nacre_string_value=\"$(printf /tmp/nacre/file.txt)\" || exit $?"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "file.txt|/tmp/nacre|.config|.json|true|file.txt|/tmp/nacre|file|.txt|true\n"
    );
}

#[test]
fn generated_bash_runs_else_if_blocks() {
    let root = temp_path("else-if");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
let count = 1
if count == 0 {
$sh"printf zero"
} else if count == 1 {
$sh"printf one"
} else if count == 2 {
$sh"printf two"
} else {
$sh"printf many"
}
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("else-if.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("else\nif awk"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "one");
}

#[test]
fn generated_bash_runs_else_if_expression() {
    let root = temp_path("else-if-expression");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
let count = 2
const label = if count == 0 { "zero" } else if count == 1 { "one" } else if count == 2 { "two" } else { "many" }
try $sh"printf '%s' \"${label}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("else-if-expression.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("readonly label=$(if awk"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "two");
}

#[test]
fn generated_bash_runs_block_statement() {
    let root = temp_path("block-statement");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
let count = 0
{
const label = "inside"
count = count + 1
try $sh"printf '%s:%s\n' \"${label}\" \"${count}\""
}
try $sh"printf '%s\n' \"${count}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("block-statement.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "inside:1\n1\n");

    let leaked = nacre::compile_source(
        r#"
{
const inner = "hidden"
}
const leaked = inner
"#,
    )
    .unwrap_err();
    assert!(leaked.message().contains("undefined variable `inner`"));
}

#[test]
fn compile_file_resolves_std_process_module() {
    let root = temp_path("std-process");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    let canonical_root = fs::canonicalize(&root).unwrap();
    let exit_marker = root.join("exit-marker.txt");
    let signal_marker = root.join("signal-marker.txt");
    fs::write(
        &main,
        r#"
use std.process
use std.fs
const envName = "NACRE_PROCESS_TEST"
const envValue = process.env(envName)
const missingEnv = process.env("NACRE_PROCESS_MISSING")
const invalidEnv = process.env("BAD-NAME")
const execOut = process.exec("printf '%s' exec-ok")
const hasShell = process.hasCommand("sh")
const missingCommand = "nacre-definitely-missing-command"
const hasMissing = process.hasCommand(missingCommand)
const targetDir = "__TARGET_DIR__"
process.chdir(targetDir)
const changedDir = process.cwd()
const exitMarker = "__EXIT_MARKER__"
const signalMarker = "__SIGNAL_MARKER__"
const procArgs = process.args()
const argCount = procArgs.len()
const [command, ...rest] = process.args()
let collected = ""
for item in process.args() {
collected = collected ++ item ++ ";"
}
fn cleanupExit(): Unit {
fs.writeText(exitMarker, "exit")
}
fn cleanupSignal(): Unit {
fs.writeText(signalMarker, "signal")
}
process.onExit(cleanupExit)
process.onSignal("TERM", cleanupSignal)
$sh"kill -TERM $$"
try $sh"printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|' \"${envValue}\" \"${missingEnv}\" \"${invalidEnv}\" \"${execOut}\" \"${hasShell}\" \"${hasMissing}\" \"${changedDir}\" \"${procArgs[0]}\" \"${argCount}\" \"${command}\" \"${rest[0]}\" \"${collected}\""
process.exit(7)
try $sh"printf after"
"#
        .replace("__TARGET_DIR__", &canonical_root.to_string_lossy())
        .replace("__EXIT_MARKER__", &exit_marker.to_string_lossy())
        .replace("__SIGNAL_MARKER__", &signal_marker.to_string_lossy()),
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-process.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash")
        .arg(&script)
        .arg("run")
        .arg("target")
        .env("NACRE_PROCESS_TEST", "dynamic-env")
        .output()
        .unwrap();

    assert!(bash.contains("process.exit() {"));
    assert!(bash.contains("process.exec() {"));
    assert!(bash.contains("process.hasCommand() {"));
    assert!(bash.contains("process.cwd() {"));
    assert!(bash.contains("process.chdir() {"));
    assert!(bash.contains("process.onExit() {"));
    assert!(bash.contains("process.onSignal() {"));
    assert!(bash.contains("bash -c"));
    assert!(bash.contains("trap "));
    assert!(bash.contains("__nacre_env_name="));
    assert!(bash.contains("readonly -a procArgs=(\"${args[@]}\")"));
    assert!(bash.contains("for item in \"${args[@]}\"; do"));
    assert!(bash.contains("process.exit 7"));
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "dynamic-env|||exec-ok|true|false|{}|run|2|run|target|run;target;|",
            canonical_root.to_string_lossy()
        )
    );
    assert_eq!(fs::read_to_string(exit_marker).unwrap(), "exit");
    assert_eq!(fs::read_to_string(signal_marker).unwrap(), "signal");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn compile_file_resolves_std_cli_module() {
    let root = temp_path("std-cli");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.cli
const options = cli.parse()
const name = options["name"]
const count = options["count"]
const verbose = options.has("verbose")
const missing = options.has("missing")
try $sh"printf '%s|%s|%s|%s\n' \"${name}\" \"${count}\" \"${verbose}\" \"${missing}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-cli.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash")
        .arg(&script)
        .arg("--name")
        .arg("Ada")
        .arg("--verbose")
        .arg("--count=3")
        .arg("ignored")
        .output()
        .unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("declare -A options"));
    assert!(bash.contains("for __nacre_cli_arg in \"${args[@]}\"; do"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Ada|3|true|false\n"
    );
}

#[test]
fn compile_file_resolves_std_test_module() {
    let root = temp_path("std-test");
    fs::create_dir_all(&root).unwrap();
    let success = root.join("success.ncr");
    fs::write(
        &success,
        r#"
use std.test
test.assert(1 == 1)
test.assert("a" == "a", "strings match")
try $sh"printf ok"
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&success).unwrap();
    let script = root.join("std-test-success.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();

    assert!(bash.contains("test.assert() {"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ok");

    let failure = root.join("failure.ncr");
    fs::write(
        &failure,
        r#"
use std.test
test.assert(false, "expected failure")
try $sh"printf unreachable"
"#,
    )
    .unwrap();
    let bash = nacre::compile_file(&failure).unwrap();
    let script = root.join("std-test-failure.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "expected failure\n"
    );
}

#[test]
fn compile_file_resolves_std_log_module() {
    let root = temp_path("std-log");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.log
log.info("ready")
log.warn("careful")
log.error("failed")
log.debug("trace")
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-log.sh");
    fs::write(&script, &bash).unwrap();
    let quiet = Command::new("bash").arg(&script).output().unwrap();
    let debug = Command::new("bash")
        .arg(&script)
        .env("NACRE_DEBUG", "1")
        .output()
        .unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("log.info() {"));
    assert!(bash.contains("log.warn() {"));
    assert!(bash.contains("log.error() {"));
    assert!(bash.contains("log.debug() {"));
    assert!(quiet.status.success());
    assert_eq!(String::from_utf8(quiet.stdout).unwrap(), "INFO ready\n");
    assert_eq!(
        String::from_utf8(quiet.stderr).unwrap(),
        "WARN careful\nERROR failed\n"
    );
    assert!(debug.status.success());
    assert_eq!(String::from_utf8(debug.stdout).unwrap(), "INFO ready\n");
    assert_eq!(
        String::from_utf8(debug.stderr).unwrap(),
        "WARN careful\nERROR failed\nDEBUG trace\n"
    );
}

#[test]
fn compile_file_resolves_std_json_module() {
    let root = temp_path("std-json");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.json
const data = json.parse("{\"name\":\"Ada\",\"count\":3,\"ok\":true}")
const encoded = json.stringify(data)
const literalEncoded = json.stringify({ "city": "Kyoto", "name": "Ada" })
const directEncoded = json.stringify(json.parse("{\"role\":\"admin\"}"))
const reparsed = json.parse(encoded)
const name = data["name"]
const count = data["count"]
const ok = data["ok"]
const reparsedName = reparsed["name"]
const hasCount = reparsed.has("count")
const literalMap = json.parse(literalEncoded)
const directMap = json.parse(directEncoded)
const literalCity = literalMap["city"]
const directRole = directMap["role"]
try $sh"printf '%s|%s|%s|%s|%s|%s|%s\n' \"${name}\" \"${count}\" \"${ok}\" \"${reparsedName}\" \"${hasCount}\" \"${literalCity}\" \"${directRole}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-json.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("declare -A data"));
    assert!(bash.contains("awk 'function skip_ws()"));
    assert!(bash.contains("readonly encoded=\"$(printf '{'"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Ada|3|true|Ada|true|Kyoto|admin\n"
    );
}

#[test]
fn compile_file_resolves_std_io_module() {
    let root = temp_path("std-io");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.io
const name = io.prompt("name: ")
const ok = io.confirm("continue? ")
const password = io.promptPassword("password: ")
try $sh"printf '%s|%s|%s\n' \"${name}\" \"${ok}\" \"${password}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-io.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash")
        .arg(&script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;

            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(b"Ada\nyes\nsecret\n")?;
            child.wait_with_output()
        })
        .unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("io.prompt() {"));
    assert!(bash.contains("io.confirm() {"));
    assert!(bash.contains("io.promptPassword() {"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Ada|true|secret\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "name: continue? password: "
    );
}

#[test]
fn compile_file_resolves_std_fs_create_temp_dir() {
    let root = temp_path("std-fs");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.fs
use std.path
const tmp = fs.createTempDir()
const file = path.join(tmp, "note.txt")
const linesFile = path.join(tmp, "lines.txt")
const copiedLinesFile = path.join(tmp, "copied-lines.txt")
const literalLinesFile = path.join(tmp, "literal-lines.txt")
const appendFile = path.join(tmp, "append.txt")
const copyFile = path.join(tmp, "copy.txt")
const movedFile = path.join(tmp, "moved.txt")
const touchedFile = path.join(tmp, "touched.txt")
fs.writeText(file, "hello")
fs.writeText(linesFile, "left\nright")
fs.writeText(appendFile, "start\n")
fs.appendText(appendFile, "middle\n")
fs.appendLines(appendFile, ["end", "tail"])
fs.copy(file, copyFile)
fs.move(copyFile, movedFile)
fs.touch(touchedFile)
const content = fs.readText(file)
const lines = fs.readLines(linesFile)
const appendedLines = fs.readLines(appendFile)
const lineCount = lines.len()
let loopedLines = ""
for line in fs.readLines(linesFile) {
loopedLines = loopedLines ++ line ++ ","
}
fs.writeLines(copiedLinesFile, lines)
fs.writeLines(literalLinesFile, ["one", "two"])
const copiedLines = fs.readLines(copiedLinesFile)
const literalLines = fs.readLines(literalLinesFile)
const movedContent = fs.readText(movedFile)
const existsBefore = fs.exists(file)
const fileIsFile = fs.isFile(file)
const tmpIsDir = fs.isDir(tmp)
const fileSize = fs.size(file)
const entries = fs.list(tmp)
const entryCount = entries.len()
const baseName = fs.basename(file)
const dirName = fs.dirname(file)
const dirMatches = dirName == tmp
const fileStem = fs.stem(file)
const fileExt = fs.extname(file)
const noExt = fs.extname("README")
let entryLoopCount = 0
for entry in fs.list(tmp) {
entryLoopCount = entryLoopCount + 1
}
const copyExists = fs.exists(copyFile)
const movedExists = fs.exists(movedFile)
const touchedExists = fs.exists(touchedFile)
fs.remove(tmp)
const existsAfter = fs.exists(tmp)
try $sh"printf '%s\n' \"${content}|${lines[0]}|${lines[1]}|${lineCount}|${loopedLines}|${copiedLines[0]}|${copiedLines[1]}|${literalLines[0]}|${literalLines[1]}|${appendedLines[0]}|${appendedLines[1]}|${appendedLines[2]}|${appendedLines[3]}|${movedContent}|${existsBefore}|${fileIsFile}|${tmpIsDir}|${fileSize}|${entryCount}|${entryLoopCount}|${baseName}|${dirMatches}|${fileStem}|${fileExt}|${noExt}|${copyExists}|${movedExists}|${touchedExists}|${existsAfter}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-fs.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("fs.createTempDir() {"));
    assert!(bash.contains("fs.isFile() {"));
    assert!(bash.contains("fs.isDir() {"));
    assert!(bash.contains("fs.size() {"));
    assert!(bash.contains("fs.copy() {"));
    assert!(bash.contains("fs.move() {"));
    assert!(bash.contains("fs.touch() {"));
    assert!(bash.contains("fs.basename() {"));
    assert!(bash.contains("fs.dirname() {"));
    assert!(bash.contains("fs.stem() {"));
    assert!(bash.contains("fs.extname() {"));
    assert!(bash.contains("fs.appendText() {"));
    assert!(bash.contains("fs.appendLines() {"));
    assert!(bash.contains("readonly tmp=\"$(fs.createTempDir)\""));
    assert!(bash.contains("mapfile -t lines < \"$linesFile\""));
    assert!(bash.contains("done < \"$linesFile\""));
    assert!(bash
        .contains("mapfile -t entries < <(find \"$tmp\" -mindepth 1 -maxdepth 1 -print | sort)"));
    assert!(bash.contains("done < <(find \"$tmp\" -mindepth 1 -maxdepth 1 -print | sort)"));
    assert!(!bash.contains("for entry in $(find "));
    assert!(bash.contains("printf '%s\\n' \"${lines[@]}\" > \"$copiedLinesFile\""));
    assert!(bash.contains("printf '%s\\n' 'one' 'two' > \"$literalLinesFile\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hello|left|right|2|left,right,|left|right|one|two|start|middle|end|tail|hello|true|true|true|5|7|7|note.txt|true|note|.txt||false|true|true|false\n",
    );
}

#[test]
fn compile_file_resolves_std_str_module() {
    let root = temp_path("std-str");
    fs::create_dir_all(&root).unwrap();
    let main = root.join("main.ncr");
    fs::write(
        &main,
        r#"
use std.str
const padded = " nacre "
const clean = str.trim(padded)
const cleanLeft = str.trimStart(padded)
const cleanRight = str.trimEnd(padded)
const cleanLen = str.len(clean)
const cleanEmpty = str.isEmpty(clean)
const parts = str.split("left,middle,right", ",")
const joined = str.join(parts, "|")
const middle = str.slice(joined, 5, 11)
const hasMid = str.contains(joined, "middle")
const midIndex = str.indexOf(joined, "middle")
const starts = str.startsWith(joined, "left")
const ends = str.endsWith(joined, "right")
const upper = str.toUpper("Nacre")
const lower = str.toLower("NACRE")
const repeated = str.repeat("na", 3)
const replaced = str.replace(joined, "middle", "center")
try $sh"printf '%s\n' \"${clean}|${cleanLeft}|${cleanRight}|${cleanLen}|${cleanEmpty}|${parts[1]}|${joined}|${middle}|${hasMid}|${midIndex}|${starts}|${ends}|${upper}|${lower}|${repeated}|${replaced}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("std-str.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("str.split() {"));
    assert!(bash.contains("str.join() {"));
    assert!(bash.contains("str.len() {"));
    assert!(bash.contains("str.isEmpty() {"));
    assert!(bash.contains("str.slice() {"));
    assert!(bash.contains("str.trim() {"));
    assert!(bash.contains("str.trimStart() {"));
    assert!(bash.contains("str.trimEnd() {"));
    assert!(bash.contains("str.contains() {"));
    assert!(bash.contains("str.indexOf() {"));
    assert!(bash.contains("str.startsWith() {"));
    assert!(bash.contains("str.endsWith() {"));
    assert!(bash.contains("str.toUpper() {"));
    assert!(bash.contains("str.toLower() {"));
    assert!(bash.contains("str.repeat() {"));
    assert!(bash.contains("str.replace() {"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nacre|nacre | nacre|5|false|middle|left|middle|right|middle|true|5|true|true|NACRE|nacre|nanana|left|center|right\n"
    );
}

#[test]
fn compile_file_namespaces_imported_function_references() {
    let root = temp_path("module-fnref");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    let module = lib.join("utils.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
fn exclaim(value: String): String {
return "${value}!"
}
fn apply(f: String => String, value: String): String {
return f(value)
}
fn loud(value: String): String {
return apply(exclaim, value)
}
fn set(key: String, value: String): String {
return "${key}:${value}"
}
fn map(value: String): String {
return "${value}!"
}
fn flatMap(value: String): String {
return "${value}?"
}
fn orElse(value: String): String {
return "${value}:or"
}
fn ap(value: String): String {
return "${value}:ap"
}
fn mapMaybe(value: String?): String? {
return value.map(exclaim)
}
fn okExclaim(value: String): String \/ String {
return Ok(exclaim(value))
}
fn mapResult(value: String \/ String): String \/ String {
return value.map(exclaim)
}
fn flatMapResult(value: String \/ String): String \/ String {
return value.flatMap(okExclaim)
}
fn lambdaExclaim(value: String): String {
const transform: String => String = item => exclaim(item)
return transform(value)
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.utils
const callback = utils.exclaim
const message = utils.loud("module")
const direct = callback("direct")
const setMessage = utils.set("key", "value")
const mapMessage = utils.map("mapped")
const flatMapMessage = utils.flatMap("flat")
const orElseMessage = utils.orElse("alternate")
const apMessage = utils.ap("apply")
const maybeMessage = utils.mapMaybe(Some("maybe")) ?? "missing"
const resultMessage = utils.mapResult(Ok("result")) ?? "missing"
const flatResultMessage = utils.flatMapResult(Ok("flat-result")) ?? "missing"
const lambdaMessage = utils.lambdaExclaim("lambda")
try $sh'echo "${message}"'
try $sh'echo "${direct}"'
try $sh'echo "${setMessage}"'
try $sh'echo "${mapMessage}"'
try $sh'echo "${flatMapMessage}"'
try $sh'echo "${orElseMessage}"'
try $sh'echo "${apMessage}"'
try $sh'echo "${maybeMessage}"'
try $sh'echo "${resultMessage}"'
try $sh'echo "${flatResultMessage}"'
try $sh'echo "${lambdaMessage}"'
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("module-fnref.sh");
    fs::write(&script, bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "module!\ndirect!\nkey:value\nmapped!\nflat?\nalternate:or\napply:ap\nmaybe!\nresult!\nflat-result!\nlambda!\n"
    );
}

#[test]
fn compile_file_resolves_external_definition_modules() {
    let root = temp_path("external-def");
    fs::create_dir_all(&root).unwrap();
    let definition = root.join("libexternal.d.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &definition,
        r#"
export fn echo(value: String): String
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use libexternal
raw {
libexternal.echo() { printf 'ext:%s' "$1"; }
}
const message = libexternal.echo("ok")
try $sh'printf "%s\n" "${message}"'
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("external-def.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert_eq!(bash.matches("libexternal.echo() {").count(), 1);
    assert!(bash.contains("readonly message=\"$(libexternal.echo 'ok')\""));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}\nbash:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bash
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ext:ok\n");
}

#[test]
fn compile_file_namespaces_imported_bindings() {
    let root = temp_path("module-binding");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    let module = lib.join("utils.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
fn passthrough(prefix: String): String {
return prefix
}
const _ = "discarded"
const prefix = "module"
const parts = ["alpha", "beta"]
const [firstPart, ...remainingParts] = parts
const remainingFirst = remainingParts[0]
const suffix = parts[1]
const pair = ("tuple", 7)
const (tupleLabel, tupleCount) = pair
const record = { marker: "record", code: 9 }
const { code } = record
fn label(value: String): String {
return "${prefix} ${value} ${suffix} ${firstPart} ${remainingFirst} ${tupleLabel} ${tupleCount} ${code}"
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.utils
const prefix = "main"
const message = utils.label("value")
const local = utils.passthrough("arg")
try $sh'echo "${prefix}"'
try $sh'echo "${message}"'
try $sh'echo "${local}"'
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("module-binding.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("readonly utils_prefix='module'"));
    assert!(bash.contains("readonly utils_firstPart=\"${utils_parts[0]}\""));
    assert!(bash.contains("readonly -a utils_remainingParts=(\"${utils_parts[@]:1}\")"));
    assert!(bash.contains("readonly utils_tupleLabel=\"$utils_pair_1\""));
    assert!(bash.contains("readonly utils_code=\"$utils_record_code\""));
    assert!(!bash.contains("utils__"));
    assert!(bash.contains("readonly prefix='main'"));
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "main\nmodule value beta alpha beta tuple 7 9\narg\n"
    );
}

#[test]
fn compile_file_hides_private_module_declarations() {
    let root = temp_path("module-private");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    let module = lib.join("utils.ncr");
    let main = root.join("main.ncr");
    let leak = root.join("leak.ncr");
    fs::write(
        &module,
        r#"
const _secret = "hidden"
fn _decorate(value: String): String {
return "${_secret}:${value}"
}
fn label(value: String): String {
return _decorate(value)
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.utils
const message = utils.label("ok")
try $sh"printf '%s\n' \"${message}\""
"#,
    )
    .unwrap();
    fs::write(
        &leak,
        r#"
use lib.utils
const message = utils._decorate("ok")
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("module-private.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    let error = nacre::compile_file(&leak).unwrap_err();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("utils.__nacre-private_decorate() {"));
    assert!(bash.contains("readonly __nacre_private_utils_secret='hidden'"));
    assert!(!bash.contains("utils._decorate() {"));
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hidden:ok\n");
    assert!(
        error.message().contains("undefined variable `utils`"),
        "{error}"
    );
}

#[test]
fn compile_file_namespaces_imported_types_and_newtypes() {
    let root = temp_path("module-types");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    let module = lib.join("models.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
type Name = String
newtype UserId = Int
fn makeId(value: Int): UserId {
return UserId(value)
}
fn label(name: Name, id: UserId): String {
const raw: Int = id.value
return "${name}:${raw}"
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.models
type Name = Int
const local: Name = 42
const name: models.Name = "Ada"
const id: models.UserId = models.UserId(7)
const made = models.makeId(8)
const message = models.label(name, id)
const madeMessage = models.label(name, made)
try $sh"printf '%s\n' \"${local}|${message}|${madeMessage}\""
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("module-types.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("models.makeId() {"));
    assert!(bash.contains("models.label() {"));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "42|Ada:7|Ada:8\n"
    );
}

#[test]
fn compile_file_namespaces_imported_control_flow_locals() {
    let root = temp_path("module-control-flow");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    let module = lib.join("utils.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
const prefix = "module"
fn flow(): String {
const values = ["a", "b"]
let result = prefix
if values.len() > 1 {
const first = values[0]
result = "${result}:${first}"
} else {
result = "${result}:none"
}
let count = 1
while count > 0 {
count = count - 1
}
for value in values {
const current = value
result = "${result}:${current}"
}
return result
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.utils
const out = utils.flow()
try $sh'echo "${out}"'
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("module-control-flow.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("readonly utils_prefix='module'"));
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "module:a:a:b\n");
}

#[test]
fn compile_file_namespaces_imported_complex_expressions() {
    let root = temp_path("module-complex-expr");
    let lib = root.join("lib");
    fs::create_dir_all(&lib).unwrap();
    let module = lib.join("utils.ncr");
    let main = root.join("main.ncr");
    fs::write(
        &module,
        r#"
const name = "module"
const names = [name, "tail"]
const envs = { "k": name }
const pair = (name, 1)
const user = { label: name }
const first = names[0]
const count = names.len()
const mapped = envs["k"]
const tupleName = pair._1
const fieldName = user.label
const selected = if count > 1 { first } else { "none" }
const selectedByIndex = if names[0] == "tail" { first } else { "none" }
const selectedByCall = if hasCommand("sh") { first } else { "none" }
const matched = match selected { "module" => "hit", _ => "miss" }
const hasTmp = pathExists("/tmp")
fn echoValue(value: String): String {
return "${value}:${mapped}:${tupleName}:${fieldName}:${matched}:${hasTmp}:${selectedByIndex}:${selectedByCall}"
}
fn maybeLabel(value: String?): String? {
return do {
local <- value
const suffix = "!"
pure("${name}:${local}${suffix}")
}
}
"#,
    )
    .unwrap();
    fs::write(
        &main,
        r#"
use lib.utils
const name = "main"
const out = utils.echoValue(name)
const maybeOut = utils.maybeLabel(Some("value")) ?? "missing"
try $sh'echo "${out}"'
try $sh'echo "${maybeOut}"'
"#,
    )
    .unwrap();

    let bash = nacre::compile_file(&main).unwrap();
    let script = root.join("module-complex-expr.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(bash.contains("readonly utils_name='module'"));
    assert!(bash.contains("readonly name='main'"));
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "main:module:module:module:hit:true:none:module\nmodule:value!\n"
    );
}

#[test]
fn generated_function_local_bindings_do_not_leak_between_direct_calls() {
    let source = r#"
fn shout(value: String): Unit {
const message = "${value}!"
try $sh"echo ${message}"
}
shout("one")
shout("two")
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("function-local.sh");
    fs::write(&script, bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "one!\ntwo!\n");
}

#[test]
fn generated_function_locals_support_structured_and_async_bindings() {
    let source = r#"
fn localData(seed: String): String {
const names = [seed, "tail"]
let mutable = ["old"]
mutable = ["new", seed]
const envs: Map[String, String] = { "k": seed }
const pair: (String, Int) = (seed, 7)
const user: { name: String, age: Int } = { name: seed, age: 9 }
const required = try $sh"printf required"
const future = async $sh"printf async"
const asyncOut = await future
const first = names[0]
const mapped = envs["k"]
const tupleName = pair._1
const userName = user.name
const count = mutable.len()
return "${first} ${mapped} ${tupleName} ${userName} ${required} ${asyncOut} ${count}"
}
const out = localData("seed")
try $sh'echo "${out}"'
"#;
    let bash = nacre::compile_source(source).unwrap();
    let script = temp_path("function-structured-locals.sh");
    fs::write(&script, &bash).unwrap();
    let output = Command::new("bash").arg(&script).output().unwrap();
    fs::remove_file(&script).unwrap();

    assert!(bash.contains("local -A __nacre_local_localData_3_envs="));
    assert!(bash.contains("local -a __nacre_local_localData_2_mutable="));
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "seed seed seed seed required async 2\n"
    );
}

#[test]
fn public_api_covers_supported_error_paths() {
    let cases = [
        ("raw {\necho nope\n", "unterminated raw block"),
        ("const x = env.home ?? \"/tmp\"", "invalid environment name"),
        ("const x = env.HOME ?? nope", "expected quoted string"),
        ("const x = process.env(1)", "process.env name"),
        ("const x = \"unterminated", "unterminated quoted string"),
        (
            "const x = $sh\"cat <<EOF\nbody\nEOF",
            "unterminated quoted string in shell command",
        ),
        ("try $sh", "expected quoted string"),
        ("try $sh{ echo nope", "unterminated shell command"),
        ("try $sh{ echo } trailing", "unexpected text after shell command"),
        (
            "export fn echo(value: String): String {\nreturn value\n}",
            "external function declarations must not include bodies",
        ),
        (
            "export fn echo[T](value: T): T",
            "external functions cannot declare type parameters",
        ),
        (
            "export fn echo(value: String = \"x\"): String",
            "external function parameter `value` cannot have a default",
        ),
        ("for  in xs {\n}", "expected assignment"),
        ("require(nope)", "expected quoted string"),
        ("require(\"sh\", \">= 1\")", "version must use"),
        (
            "require(\"sh\", label = \">= 1\")",
            "optional argument must be `version`",
        ),
        ("requireOneOf([])", "at least one command"),
        ("requireOneOf([1])", "array of quoted strings"),
        ("try $sh nope", "expected quoted string"),
        ("not an assignment", "expected assignment"),
        ("x\\=y = 1", "invalid variable name"),
        ("const x = missing", "undefined variable"),
        ("const x = 1\nconst x = 2", "already defined"),
        ("const x = 1\nx = 2", "cannot assign to const"),
        ("x = 1", "cannot assign to undefined variable"),
        ("let x = 1\nx = true", "type mismatch"),
        ("const x: Bool = 1", "type annotation mismatch"),
        ("const x: String | Int = true", "type annotation mismatch"),
        ("const x: String & Path = 1", "type annotation mismatch"),
        ("const x: Nope = 1", "unknown type"),
        ("const x: ExitCode = 256", "type annotation mismatch"),
        ("const x = \"hello ${missing}\"", "undefined variable"),
        ("const x = \"hello ${bad-name}\"", "invalid interpolation name"),
        ("const x = \"hello ${missing\"", "unterminated string interpolation"),
        ("const x = [1, true]", "array elements"),
        ("const x: [String] = [1]", "type annotation mismatch"),
        ("const [a] = 1", "array destructuring requires array value"),
        (
            "const [a, ...rest, b] = [1, 2, 3]",
            "array rest destructuring must be last",
        ),
        ("const (a, b) = 1", "tuple destructuring requires tuple value"),
        (
            "const (a, b) = (1, 2, 3)",
            "tuple destructuring expected 2 values",
        ),
        (
            "const { missing } = { name: \"Ada\" }",
            "record destructuring field `missing` is missing",
        ),
        ("const x = missingFn()", "undefined function"),
        ("return 1", "return is only valid inside a function"),
        ("fn greet(name: String): String {\n$sh'echo no return'\n}", "must return String"),
        ("fn greet(name: String): Int {\nreturn name\n}", "return type mismatch"),
        (
            "const value = Ok(1)!",
            "only valid inside a Result-returning function",
        ),
        (
            "fn bad(): Int \\/ String {\nconst value = 1!\nreturn value\n}",
            "expects Result value",
        ),
        (
            "fn step(): Int \\/ String {\nreturn 1\n}\nfn apply(f: Int => Int): Int {\nreturn f(1)\n}\nfn bad(): Int \\/ String {\nreturn apply(value => value + step()!)\n}",
            "only valid inside a Result-returning function",
        ),
        (
            "fn select(value: String): String {\nreturn value\n}\nfn __nacre_keyword_select(value: String): String {\nreturn value\n}",
            "after Bash name mangling",
        ),
        (
            "type State = Ready | Failed(String)\nconst value: State = Failed(1)",
            "argument 1 for variant `Failed`",
        ),
        (
            "type State = Ready | Failed(String)\nconst value = Failed()",
            "variant `Failed` expects 1 arguments",
        ),
        (
            "type State = Ready | Failed(String)\nconst value: State = Ready\nconst label = match value { Ready => \"ready\" }",
            "missing cases: Failed",
        ),
        (
            "const value: Int? = Some(1)\nconst label = match value { Some(item) => item }",
            "missing cases: None",
        ),
        (
            "const value: Int \\/ String = Ok(1)\nconst label = match value { Ok(item) if item > 0 => item, Err(_) => 0 }",
            "missing cases: Ok",
        ),
        (
            "const label = match true { true => \"yes\" }",
            "missing cases: false",
        ),
        (
            "const label = match 1 { 1 => \"one\" }",
            "requires wildcard `_` arm",
        ),
        (
            "type State = Ready | Ready",
            "variant `Ready` is already defined",
        ),
        ("fn greet(name: String): String {\nreturn name\n}\nconst x = greet(1)", "argument `name`"),
        (
            "fn join(values: ...String, suffix: String): String {\nreturn suffix\n}",
            "rest parameter must be last",
        ),
        (
            "fn join(prefix: String = \"x\", values: ...String): String {\nreturn prefix\n}",
            "rest parameters cannot follow default parameters",
        ),
        (
            "fn join(values: ...String): String {\nreturn values[0]\n}\nconst x = join(1)",
            "rest argument",
        ),
        ("fn exclaim(value: String): String {\nreturn value\n}\nconst x: Int => String = exclaim", "type annotation mismatch"),
        ("fn apply(f: String => String): String {\nreturn f(1)\n}", "argument 1"),
        ("const x = 1\nconst y = x()", "not callable"),
        ("fn missingParams: String {\nreturn \"x\"\n}", "expected function parameters"),
        ("fn missingReturn(): {\nreturn \"x\"\n}", "expected type name"),
        (
            "fn badTypeParam[](value: Int): Int {\nreturn value\n}",
            "expected array element",
        ),
        ("fn badTypeParam[T(value: Int): Int {\nreturn value\n}", "unterminated function type parameters"),
        (
            "fn first[T](a: T, b: T): T {\nreturn a\n}\nconst x = first(1, true)",
            "generic type `T`",
        ),
        (
            "fn firstArray[T](value: [T]): T {\nreturn value[0]\n}\nconst x = firstArray(1)",
            "expected T, found Int",
        ),
        (
            "fn mapValue[T](value: Map[String, T]): T {\nreturn value[\"k\"]\n}\nconst x = mapValue(1)",
            "expected Map[String, T], found Int",
        ),
        (
            "fn tupleFirst[T](value: (T, String)): T {\nreturn value._1\n}\nconst x = tupleFirst((1, 2))",
            "expected String, found Int",
        ),
        (
            "fn recordItem[T](value: { item: T }): T {\nreturn value.item\n}\nconst x = recordItem({ other: 1 })",
            "record field `item` is missing",
        ),
        (
            "fn identityInt(value: Int): Int {\nreturn value\n}\nfn applySame[T](f: T => T, value: T): T {\nreturn f(value)\n}\nconst x = applySame(identityInt, \"no\")",
            "generic type `T`",
        ),
        (
            "fn pair[T](a: T, b: T): T {\nreturn a\n}\nconst x = pair([1], [\"x\"])",
            "generic type `T`",
        ),
        ("trait Show {\n}", "expected type parameters"),
        ("trait Show[T, U] {\n}", "requires exactly one type parameter"),
        ("trait Show[T] {\nconst x = 1\n}", "trait bodies support method signatures only"),
        ("trait Show[T] {\nfn show[T](value: T): String\n}", "trait methods cannot declare type parameters"),
        ("trait Show[T] {\nfn show(value: T = \"x\"): String\n}", "trait methods cannot declare default parameters"),
        ("trait Show[T] {\nfn show(value: T): String {\n}\n}", "trait method signatures must not include bodies"),
        ("fn first[T: Show](value: T): T {\nreturn value\n}", "unknown trait `Show`"),
        ("impl Show {\n}", "expected trait implementation"),
        ("impl Show[Int, String] {\n}", "trait implementation requires exactly one type"),
        ("impl Show[Int] {\n}", "unknown trait `Show`"),
        ("trait Show[T] {\n}\nfn first[T: Show](value: T): T {\nreturn value\n}\nconst x = first(1)", "does not implement trait `Show`"),
        ("trait Show[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\n}", "missing method `show`"),
        ("trait Show[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\nfn show[T](value: Int): String {\nreturn \"x\"\n}\n}", "impl methods cannot declare type parameters"),
        ("trait Show[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\nconst x = 1\n}", "impl bodies support method definitions only"),
        ("trait Show[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\nfn extra(value: Int): String {\nreturn \"x\"\n}\n}", "is not declared by trait `Show`"),
        ("trait Show[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\nfn show(value: Int): String {\nreturn \"x\"\n}\nfn show(value: Int): String {\nreturn \"x\"\n}\n}", "impl method `show` is already defined"),
        ("trait Show[T] {\nfn show(value: T, label: String): String\n}\nimpl Show[Int] {\nfn show(value: Int): String {\nreturn \"x\"\n}\n}", "parameter count mismatch"),
        ("trait Show[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\nfn show(value: String): String {\nreturn value\n}\n}", "type mismatch"),
        ("trait Show[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\nfn show(value: Int): Int {\nreturn value\n}\n}", "return type mismatch"),
        ("trait Show[T] {\nfn show(value: String): String\n}", "receiver must be `T`"),
        ("trait Show[T] {\nfn show(): String\n}", "requires a receiver parameter"),
        ("trait Show[T] {\nfn show(value: T): String\n}\ntrait Debug[T] {\nfn show(value: T): String\n}\nimpl Show[Int] {\nfn show(value: Int): String {\nreturn \"show\"\n}\n}\nimpl Debug[Int] {\nfn show(value: Int): String {\nreturn \"debug\"\n}\n}\nconst value = 1\nconst x = value.show()", "ambiguous method `show`"),
        ("trait Show[T] {\n}\nimpl Show[Int] {\n}\nimpl Show[Int] {\n}", "already implemented"),
        ("fn exclaim(value: String): String {\nreturn value\n}\nconst value = 1\nconst x = value.exclaim()", "argument `value`"),
        ("const value = \"x\"\nconst x = value.missing()", "undefined function `missing`"),
        ("type Box[T] = { item: T }\nconst x: Box = { value: 1 }", "unknown type"),
        ("type Box[T] = { item: T }\nconst x: Box[Int, String] = { value: 1 }", "expects 1 type arguments"),
        ("const x = await missing", "undefined future"),
        ("const x = 1\nconst y = await x", "await expects Future"),
        ("const x = 1\nconst y = x.wait()", "await expects Future"),
        ("const x = 1 |> $sh\"cat\"", "pipeline input must be String or Path"),
        ("const x = 1 ?? 2", "requires Option or Result value"),
        ("const x: String? = Some(1)", "type annotation mismatch"),
        ("const x = Some(1) ?? \"fallback\"", "fallback mismatch"),
        ("const x: String \\/ String = Ok(1)", "type annotation mismatch"),
        ("const x: String \\/ String = Err(1)", "type annotation mismatch"),
        ("const x = Ok(1) ?? \"fallback\"", "fallback mismatch"),
        ("const x = Some(1)?", "requires Result value"),
        (
            "const x = match 1 { Some(v) => v, _ => 0 }",
            "match pattern type mismatch",
        ),
        (
            "const x = match Some(1) { Some(v) => v, _ => \"x\" }",
            "match arms",
        ),
        (
            "const x = match Some(1) { Some(v) if v => v, _ => 0 }",
            "condition must be Bool",
        ),
        (
            "const x = match Some(1) { _ if true => 1 }",
            "missing cases: Some, None",
        ),
        ("const x = if 1 { \"a\" } else { \"b\" }", "condition must be Bool"),
        ("const x = if true { 1 } else { \"b\" }", "if expression branches"),
        (
            "const x = if if true { true } else { false } { 1 } else { 0 }",
            "unexpected text after if expression",
        ),
        ("const x = match 1 { 1 => \"one\" }", "wildcard `_` arm"),
        ("const x = match 1 { \"one\" => 1, _ => 0 }", "match pattern type mismatch"),
        ("const x = match 1 { 1 => 1, _ => \"zero\" }", "match arms"),
        (
            "const x = match (1, \"a\") { (1, \"a\", \"b\") => \"one\", _ => \"other\" }",
            "match pattern type mismatch",
        ),
        (
            "const x = match 1 { (1, \"a\") => \"hit\", _ => \"miss\" }",
            "match pattern type mismatch",
        ),
        (
            "const user = { name: \"Ada\" }\nconst x = match user { { missing } => missing, _ => \"none\" }",
            "match record pattern field `missing` is missing",
        ),
        ("const x: Map[String] = {}", "Map type requires"),
        ("const x = {", "unterminated map literal"),
        ("const x = { \"a\" }", "expected `:` in map entry"),
        ("const x = { \"a\": }", "expected map key and value"),
        ("const m = { \"a\": 1 }\nconst x = m[1]", "map key must be String"),
        ("const x = { \"a\": 1, 2: 2 }", "map keys"),
        ("const x = { \"a\": 1, \"b\": true }", "map values"),
        ("const x: Map[String, String] = { \"a\": 1 }", "type annotation mismatch"),
        ("const x = { name: \"Ada\", name: \"Grace\" }", "record field `name`"),
        ("const x: { name: String, age: Int } = { name: \"Ada\" }", "type annotation mismatch"),
        ("const x = { name: \"Ada\" }\nconst y = x.age", "has no field `age`"),
        ("const x = 1\nconst y = x.name", "cannot access field `name`"),
        (
            "const x = ({ name: \"Ada\" }).age",
            "record value has no field `age`",
        ),
        (
            "const x = (1).name",
            "cannot access field `name` on value of type Int",
        ),
        ("type User = { name: String }\ntype User = { name: String }", "already defined"),
        ("type User = Missing", "unknown type"),
        ("const x = missing[0]", "undefined variable"),
        ("const xs = [1]\nconst x = xs[true]", "array index must be Int"),
        ("const x = 1\nconst y = x[0]", "cannot index"),
        (
            "const x = ([1])[true]",
            "array index must be Int",
        ),
        (
            "const x = ({ \"a\": 1 })[1]",
            "map key must be String",
        ),
        ("const x = (1)[0]", "cannot index value of type Int"),
        ("const x = 1\nconst y = x.len()", "has no len method"),
        ("const x = (1).len()", "type Int has no len method"),
        (
            "const x = try $sh\"printf x\".len(1)",
            "len expects no arguments",
        ),
        ("const x = 1\nconst y = x.isEmpty()", "has no isEmpty method"),
        ("const x = (1).isEmpty()", "type Int has no isEmpty method"),
        (
            "const x = try $sh\"printf x\".isEmpty(1)",
            "isEmpty expects no arguments",
        ),
        ("const x = 1\nconst y = x.first()", "has no first method"),
        ("const x = (1).first()", "type Int has no first method"),
        (
            "const x = ([]).first()",
            "first requires a non-empty array literal",
        ),
        ("const x = 1\nconst y = x.last()", "has no last method"),
        ("const x = (1).last()", "type Int has no last method"),
        (
            "const x = ([]).last()",
            "last requires a non-empty array literal",
        ),
        ("const x = 1\nconst y = x.reverse()", "has no reverse method"),
        ("const x = (1).reverse()", "type Int has no reverse method"),
        ("const x = 1\nconst y = x.sort()", "has no sort method"),
        ("const x = (1).sort()", "type Int has no sort method"),
        ("const xs = [1]\nconst y = xs.sort()", "sort array elements"),
        ("const y = ([1]).sort()", "sort array elements"),
        ("const x = 1\nconst y = x.unique()", "has no unique method"),
        ("const x = (1).unique()", "type Int has no unique method"),
        ("const x = 1\nconst y = x.map(value => value)", "has no map method"),
        (
            "const x = (1).map(value => value)",
            "type Int has no map method",
        ),
        (
            "const x = ([]).map(value => value)",
            "map requires a non-empty array literal or typed array",
        ),
        (
            "const xs = [1]\nconst y = xs.map((left, right) => left)",
            "map lambda expects 1 parameter",
        ),
        (
            "const xs = [1]\nconst y = xs.map(1)",
            "map mapper must be a function",
        ),
        (
            "fn text(value: String): String {\nreturn value\n}\nconst xs = [1]\nconst y = xs.map(text)",
            "map mapper parameter must accept Int",
        ),
        (
            "const value = None\nconst mapped = value.map(item => item)",
            "map on None requires a typed Option",
        ),
        (
            "const value: Int? = Some(1)\nconst mapped = value.map((left, right) => left)",
            "map lambda expects 1 parameter",
        ),
        (
            "const value: Int? = Some(1)\nconst mapped = value.map(1)",
            "map mapper must be a function",
        ),
        (
            "fn text(value: String): String {\nreturn value\n}\nconst value: Int? = Some(1)\nconst mapped = value.map(text)",
            "map mapper parameter must accept Int",
        ),
        (
            "const value = 1\nconst mapped = value.flatMap(item => Some(item))",
            "has no flatMap method",
        ),
        (
            "const value = None\nconst mapped = value.flatMap(item => Some(item))",
            "flatMap on None requires a typed Option",
        ),
        (
            "const value: Int? = Some(1)\nconst mapped = value.flatMap((left, right) => Some(left))",
            "flatMap lambda expects 1 parameter",
        ),
        (
            "const value: Int? = Some(1)\nconst mapped = value.flatMap(1)",
            "flatMap mapper must be a function",
        ),
        (
            "const value: Int? = Some(1)\nconst mapped = value.flatMap(item => item + 1)",
            "flatMap mapper must return Option",
        ),
        (
            "fn text(value: String): String? {\nreturn Some(value)\n}\nconst value: Int? = Some(1)\nconst mapped = value.flatMap(text)",
            "flatMap mapper parameter must accept Int",
        ),
        (
            "const value: Int? = Some(1)\nconst mapped = value.flatMap(item => None)",
            "flatMap mapper returning None requires a typed Option",
        ),
        (
            "const value = Err(\"bad\")\nconst mapped = value.map(item => item)",
            "map on Err requires a typed Result",
        ),
        (
            "const value = Err(\"bad\")\nconst mapped = value.flatMap(item => Ok(item))",
            "flatMap on Err requires a typed Result",
        ),
        (
            "const value: Int \\/ String = Ok(1)\nconst mapped = value.flatMap(item => item + 1)",
            "flatMap mapper must return Result",
        ),
        (
            "fn other(value: Int): Int \\/ Int {\nreturn Err(1)\n}\nconst value: Int \\/ String = Ok(1)\nconst mapped = value.flatMap(other)",
            "flatMap mapper error must be assignable to String",
        ),
        (
            "const value = 1\nconst selected = value.orElse(Some(2))",
            "has no orElse method",
        ),
        (
            "const value: Int? = Some(1)\nconst selected = value.orElse(2)",
            "orElse fallback must be Option",
        ),
        (
            "const value: Int? = Some(1)\nconst selected = value <|> Some(\"two\")",
            "orElse fallback mismatch",
        ),
        (
            "const selected = None <|> None",
            "orElse with two None values requires a typed Option",
        ),
        (
            "const value = 1\nconst mapped = value <$> (item => item)",
            "has no map method",
        ),
        (
            "const value: Int? = Some(1)\nconst mapped = value >>= (item => item + 1)",
            "flatMap mapper must return Option",
        ),
        (
            "const value: Int \\/ String = Ok(1)\nconst mapped = value >>= (item => item + 1)",
            "flatMap mapper must return Result",
        ),
        (
            "const function = Some(1)\nconst applied = function.ap(Some(2))",
            "ap receiver must contain a function",
        ),
        (
            "fn add(left: Int, right: Int): Int {\nreturn left + right\n}\nconst function: Option[(Int, Int) => Int] = Some(add)\nconst applied = function.ap(Some(1))",
            "ap function expects 1 parameter",
        ),
        (
            "fn double(value: Int): Int {\nreturn value * 2\n}\nconst function: Option[Int => Int] = Some(double)\nconst applied = function.ap(1)",
            "ap argument must be Option",
        ),
        (
            "fn double(value: Int): Int {\nreturn value * 2\n}\nconst function: Option[Int => Int] = Some(double)\nconst applied = function <*> Some(\"two\")",
            "ap argument must contain Int",
        ),
        (
            "const function = None\nconst applied = function.ap(Some(1))",
            "ap on None requires a typed Option function",
        ),
        (
            "fn double(value: Int): Int {\nreturn value * 2\n}\nconst function: Result[Int => Int, String] = Ok(double)\nconst value: Int \\/ Int = Err(1)\nconst applied = function.ap(value)",
            "ap argument error must be assignable to String",
        ),
        ("const x = 1\nconst y = x.take(1)", "has no take method"),
        ("const x = (1).take(1)", "type Int has no take method"),
        ("const x = 1\nconst y = x.drop(1)", "has no drop method"),
        ("const x = (1).drop(1)", "type Int has no drop method"),
        ("const xs = [\"a\"]\nconst y = xs.take(\"1\")", "take count"),
        ("const y = ([\"a\"]).take(\"1\")", "take count"),
        ("const xs = [\"a\"]\nconst y = xs.drop(\"1\")", "drop count"),
        ("const y = ([\"a\"]).drop(\"1\")", "drop count"),
        ("const x = 1\nconst y = x.slice(0, 1)", "has no slice method"),
        ("const xs = [\"a\"]\nconst x = xs.slice(\"0\", 1)", "slice start"),
        ("const x = ([\"a\"]).slice(\"0\", 1)", "slice start"),
        ("const xs = [\"a\"]\nconst x = xs.slice(0, \"1\")", "slice end"),
        ("const x = ([\"a\"]).slice(0, \"1\")", "slice end"),
        (
            "const text = \"abc\"\nconst x = text.slice(\"0\", 1)",
            "slice start",
        ),
        (
            "const text = \"abc\"\nconst x = text.slice(0, \"1\")",
            "slice end",
        ),
        ("const x = (1).slice(0, 1)", "type Int has no slice method"),
        ("const x = (\"abc\").slice(\"0\", 1)", "slice start"),
        (
            "const x = try $sh\"printf abc\".slice(0)",
            "slice expects start and end arguments",
        ),
        ("const xs = [\"a\"]\nxs.push(\"b\")", "cannot push to const array"),
        ("let x = 1\nx.push(2)", "has no push method"),
        ("let xs = [\"a\"]\nxs.push(1)", "push value type mismatch"),
        (
            "let xs = [\"a\"]\nconst x = xs.push(\"b\")",
            "push is only valid as a statement",
        ),
        ("const xs = [\"a\"]\nxs.pop()", "cannot pop from const array"),
        ("let x = 1\nx.pop()", "has no pop method"),
        (
            "let xs = [\"a\"]\nconst x = xs.pop()",
            "pop is only valid as a statement",
        ),
        (
            "const envs = { \"PORT\": \"8080\" }\nenvs.set(\"PORT\", \"9090\")",
            "cannot set const map",
        ),
        ("let x = 1\nx.set(1, 2)", "has no set method"),
        (
            "let envs = { \"PORT\": \"8080\" }\nenvs.set(1, \"9090\")",
            "map key must be String",
        ),
        (
            "let envs = { \"PORT\": \"8080\" }\nenvs.set(\"PORT\", 9090)",
            "map value must be String",
        ),
        (
            "let envs = { \"PORT\": \"8080\" }\nconst x = envs.set(\"PORT\", \"9090\")",
            "set is only valid as a statement",
        ),
        (
            "const envs = { \"PORT\": \"8080\" }\nenvs.remove(\"PORT\")",
            "cannot remove from const map",
        ),
        ("let x = 1\nx.remove(1)", "has no remove method"),
        (
            "let envs = { \"PORT\": \"8080\" }\nenvs.remove(1)",
            "map key must be String",
        ),
        (
            "let envs = { \"PORT\": \"8080\" }\nconst x = envs.remove(\"PORT\")",
            "remove is only valid as a statement",
        ),
        (
            "const f = value => value",
            "lambda type cannot be inferred",
        ),
        (
            "const f: Int => Int = (left, right) => left",
            "lambda expects 1 parameters",
        ),
        (
            "const f: Int => String = value => value + 1",
            "lambda return type mismatch",
        ),
        (
            "const value = do {\nitem <- 1\npure(item)\n}",
            "do binding expects Option or Result",
        ),
        (
            "const value = do {\npure(1)\n}",
            "pure in do expression requires an Option or Result binding",
        ),
        (
            "const value = do {\nitem <- Some(1)\nOk(item)\n}",
            "flatMap mapper must return Option",
        ),
        (
            "const value = do {\nitem <- Some(1)\n}",
            "do expression must end with a result expression",
        ),
        (
            "fn apply(f: Int => Int): Int {\nreturn f(1)\n}\nconst x = apply((left, right) => left)",
            "lambda expects 1 parameters",
        ),
        (
            "const f: (Int, Int) => Int = (value, value) => value",
            "lambda parameter `value` is already defined",
        ),
        ("const x = 1\nconst y = x.join(\",\")", "has no join method"),
        ("const xs = [1]\nconst x = xs.join(\",\")", "join array elements"),
        ("const xs = [\"a\"]\nconst x = xs.join(1)", "join separator"),
        ("const x = ([1]).join(\",\")", "join array elements"),
        ("const x = ([\"a\"]).join(1)", "join separator"),
        (
            "fn make(): [String] {\nreturn [\"a\"]\n}\nconst x = make().join(\",\")",
            "array literal or named array",
        ),
        ("const x = 1\nconst y = x.keys()", "has no keys method"),
        ("const x = 1\nconst y = x.values()", "has no values method"),
        ("const x = 1\nconst y = x.has(\"a\")", "has no has method"),
        ("const m = { \"a\": 1 }\nconst y = m.has(1)", "map key must be String"),
        ("const x = (1).keys()", "has no keys method"),
        ("const x = (1).values()", "has no values method"),
        ("const x = (1).has(\"a\")", "has no has method"),
        ("const x = ({ \"a\": 1 }).has(1)", "map key must be String"),
        ("const x = 1\nconst y = x.contains(\"a\")", "has no contains method"),
        (
            "const x = (1).contains(\"a\")",
            "type Int has no contains method",
        ),
        (
            "const x = try $sh\"printf x\".contains()",
            "contains expects one needle argument",
        ),
        ("const x = \"abc\"\nconst y = x.contains(1)", "contains needle"),
        ("const x = (\"abc\").contains(1)", "contains needle"),
        (
            "const xs = [1]\nconst y = xs.contains(\"1\")",
            "contains value type mismatch",
        ),
        (
            "const y = ([1]).contains(\"1\")",
            "contains value type mismatch",
        ),
        ("const x = 1\nconst y = x.indexOf(1)", "has no indexOf method"),
        ("const x = (1).indexOf(\"a\")", "type Int has no indexOf method"),
        (
            "const x = try $sh\"printf x\".indexOf()",
            "indexOf expects one needle argument",
        ),
        (
            "const xs = [1]\nconst y = xs.indexOf(\"1\")",
            "indexOf value type mismatch",
        ),
        (
            "const y = ([1]).indexOf(\"1\")",
            "indexOf value type mismatch",
        ),
        (
            "const text = \"abc\"\nconst y = text.indexOf(1)",
            "indexOf needle",
        ),
        ("const x = (\"abc\").indexOf(1)", "indexOf needle"),
        ("const x = 1\nconst y = x.trim()", "has no trim method"),
        ("const x = (1).trim()", "type Int has no trim method"),
        (
            "const x = try $sh\"printf x\".trim(1)",
            "trim expects no arguments",
        ),
        ("const x = 1\nconst y = x.trimStart()", "has no trimStart method"),
        (
            "const x = (1).trimStart()",
            "type Int has no trimStart method",
        ),
        (
            "const x = try $sh\"printf x\".trimStart(1)",
            "trimStart expects no arguments",
        ),
        ("const x = 1\nconst y = x.trimEnd()", "has no trimEnd method"),
        ("const x = (1).trimEnd()", "type Int has no trimEnd method"),
        (
            "const x = try $sh\"printf x\".trimEnd(1)",
            "trimEnd expects no arguments",
        ),
        ("const x = 1\nconst y = x.startsWith(\"a\")", "has no startsWith method"),
        (
            "const x = (1).startsWith(\"a\")",
            "type Int has no startsWith method",
        ),
        (
            "const x = try $sh\"printf x\".startsWith()",
            "startsWith expects one prefix argument",
        ),
        ("const x = \"abc\"\nconst y = x.startsWith(1)", "startsWith prefix"),
        ("const x = (\"abc\").startsWith(1)", "startsWith prefix"),
        ("const x = 1\nconst y = x.endsWith(\"c\")", "has no endsWith method"),
        (
            "const x = (1).endsWith(\"c\")",
            "type Int has no endsWith method",
        ),
        (
            "const x = try $sh\"printf x\".endsWith()",
            "endsWith expects one suffix argument",
        ),
        ("const x = \"abc\"\nconst y = x.endsWith(1)", "endsWith suffix"),
        ("const x = (\"abc\").endsWith(1)", "endsWith suffix"),
        ("const x = 1\nconst y = x.toUpper()", "has no toUpper method"),
        ("const x = (1).toUpper()", "type Int has no toUpper method"),
        (
            "const x = try $sh\"printf x\".toUpper(1)",
            "toUpper expects no arguments",
        ),
        ("const x = 1\nconst y = x.toLower()", "has no toLower method"),
        ("const x = (1).toLower()", "type Int has no toLower method"),
        (
            "const x = try $sh\"printf x\".toLower(1)",
            "toLower expects no arguments",
        ),
        ("const x = 1\nconst y = x.repeat(3)", "has no repeat method"),
        ("const x = \"na\"\nconst y = x.repeat(\"3\")", "repeat count"),
        ("const x = (1).repeat(3)", "type Int has no repeat method"),
        ("const x = (\"na\").repeat(\"3\")", "repeat count"),
        (
            "const x = try $sh\"printf na\".repeat()",
            "repeat expects one count argument",
        ),
        (
            "const x = 1\nconst y = x.isAbsolute()",
            "has no isAbsolute method",
        ),
        ("const x = (1).isAbsolute()", "type Int has no isAbsolute method"),
        ("const x = (1).basename()", "type Int has no basename method"),
        ("const x = (1).dirname()", "type Int has no dirname method"),
        ("const x = (1).stem()", "type Int has no stem method"),
        ("const x = (1).extname()", "type Int has no extname method"),
        (
            "const x = try $sh\"printf /tmp\".basename(1)",
            "basename expects no arguments",
        ),
        ("const x = 1\nconst y = x.split(\",\")", "has no split method"),
        ("const x = \"a,b\"\nconst y = x.split(1)", "split separator"),
        ("const x = (1).split(\",\")", "type Int has no split method"),
        ("const x = (\"a,b\").split(1)", "split separator"),
        ("const x = 1\nconst y = x.replace(\"a\", \"b\")", "has no replace method"),
        ("const x = (1).replace(\"a\", \"b\")", "type Int has no replace method"),
        ("const x = (\"abc\").replace(1, \"b\")", "replace search"),
        (
            "const x = try $sh\"printf abc\".replace(\"a\")",
            "replace expects search and replacement arguments",
        ),
        ("const x = json.stringify({ 1: \"one\" })", "Map[String, String]"),
        ("const x = json.stringify({ \"one\": 1 })", "Map[String, String]"),
        (
            "fn make(): Map[String, String] {\nreturn { \"a\": \"b\" }\n}\nconst x = json.stringify(make())",
            "map literal or json.parse result",
        ),
        ("const y = fs.isFile(1)", "fs path must be String or Path"),
        ("const y = fs.isDir(1)", "fs path must be String or Path"),
        ("const y = fs.size(1)", "fs path must be String or Path"),
        ("const y = fs.readLines(1)", "fs path must be String or Path"),
        ("const y = fs.list(1)", "fs path must be String or Path"),
        ("fs.writeLines(1, [\"a\"])", "fs path must be String or Path"),
        ("fs.writeLines(\"/tmp/a\", 1)", "fs.writeLines lines must be Array"),
        (
            "fs.writeLines(\"/tmp/a\", [1])",
            "fs.writeLines lines must be [String]",
        ),
        ("fs.appendLines(1, [\"a\"])", "fs path must be String or Path"),
        (
            "fs.appendLines(\"/tmp/a\", 1)",
            "fs.appendLines lines must be Array",
        ),
        (
            "fs.appendLines(\"/tmp/a\", [1])",
            "fs.appendLines lines must be [String]",
        ),
        (
            "const x = \"abc\"\nconst y = x.replace(1, \"b\")",
            "replace search",
        ),
        (
            "const x = \"abc\"\nconst y = x.replace(\"a\", 1)",
            "replace replacement",
        ),
        ("const x: (String, Int) = (1, 2)", "type annotation mismatch"),
        ("const x = (1, true)\nconst y = x._3", "has no field _3"),
        ("const x = 1\nconst y = x._1", "cannot access tuple field"),
        ("const x = (1, true)._3", "tuple value has no field _3"),
        (
            "const x = (1)._1",
            "cannot access tuple field on value of type Int",
        ),
        ("const x: (String) = \"x\"", "tuple type requires"),
        ("const x = (1,", "unterminated tuple literal"),
        ("const x: Missing = 1", "unknown type"),
        ("newtype UserId = Int\nnewtype UserId = Int", "already defined"),
        ("newtype UserId = Int\nconst x: UserId = 1", "type annotation mismatch"),
        ("newtype UserId = Int\nconst x = UserId(true)", "newtype constructor"),
        (
            "newtype UserId = Int\nfn UserId(value: Int): UserId {\nreturn value as UserId\n}",
            "use `fn!` to override its constructor",
        ),
        (
            "fn! UserId(value: Int): UserId {\nreturn value as UserId\n}",
            "can only override an existing newtype constructor",
        ),
        (
            "newtype UserId = Int\nfn! UserId[T](value: T): UserId {\nreturn value as UserId\n}",
            "newtype constructor overrides cannot declare type parameters",
        ),
        ("const x = Missing(1)", "unknown type"),
        ("const x = 1\nconst y = x.value", "cannot access `.value`"),
        ("if 1 {\n$sh'no'\n}", "condition must be Bool"),
        ("while 1 {\n$sh'no'\n}", "condition must be Bool"),
        ("const x = 1\nfor item in x {\n$sh'no'\n}", "for loop iterable must be Array"),
        ("if true {\n$sh'no'\n", "unterminated block"),
        ("const x = [1,", "unterminated array literal"),
        ("const x = [1,]", "expected array element"),
        ("const x = 0xNOPE", "invalid integer literal"),
        ("const x = 0b102", "invalid integer literal"),
        ("const x = \"1\" + 2", "requires numeric operands"),
        ("const x = 1 == true", "matching operand types"),
        ("const x = true % 2", "requires Int operands"),
        ("const x = \"a\" < \"b\"", "requires numeric operands"),
    ];

    for (source, message) in cases {
        let error = nacre::compile_source(source).unwrap_err();
        assert!(error.message().contains(message), "{error}");
    }
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nacre-api-{unique}-{name}"))
}
