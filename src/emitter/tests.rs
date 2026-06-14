use super::local_mangling::{
    mangle_call_name, mangle_local_expr, mangle_local_name, mangle_local_statement,
    mangle_shell_interpolations, sanitize_shell_ident, LocalMangler,
};
use super::*;
use crate::{compile_source, parse, MatchArm, Type};

#[test]
fn compiles_assignments() {
    let bash = compile_source(
        r#"
const name = "Nacre"
let count = 40
count = count + 2
const home = env.HOME ?? "/tmp"
"#,
    )
    .unwrap();

    assert!(bash.contains("readonly name='Nacre'"));
    assert!(bash.contains("count=40"));
    assert!(bash.contains("count=$(awk "));
    assert!(bash.contains("readonly home=\"${HOME:-/tmp}\""));
}

#[test]
fn compiles_boolean_comparison_and_string_quoting() {
    let bash = compile_source(
        r#"
const ok = true
const nope = false
const same = "a'b" == 'a'
const less = 1 < 2
const bools = true == false
const sameFlag = ok == nope
const envTest = env.PATH ?? "/bin" == "/bin"
"#,
    )
    .unwrap();

    assert!(bash.contains("readonly ok=true"));
    assert!(bash.contains("readonly nope=false"));
    assert!(bash.contains("readonly same=$(awk "));
    assert!(bash.contains("(\"a'\\''b\" == \"a\")"));
    assert!(bash.contains("readonly less=$(awk "));
    assert!(bash.contains("(1 < 2)"));
    assert!(bash.contains("readonly bools=$(awk "));
    assert!(bash.contains("(\"true\" == \"false\")"));
    assert!(bash.contains("readonly sameFlag=$(awk -v __nacre_0=\"$ok\" -v __nacre_1=\"$nope\""));
    assert!(bash.contains("readonly envTest=$(awk -v __nacre_0=\"${PATH:-/bin}\""));
}

#[test]
fn compiles_nested_arithmetic_and_escaped_strings() {
    let bash = compile_source(
        r#"
let a = 1
let b = 2
const copied = a
let c = a + b * 3
let d = a - b / 2
const escaped = "a\"b"
const quoted = 'a\'b'
const noOp = "a \+ b"
"#,
    )
    .unwrap();

    assert!(bash.contains("readonly copied=\"$a\""));
    assert!(bash.contains("c=$(awk -v __nacre_0=\"$a\" -v __nacre_1=\"$b\""));
    assert!(bash.contains("(__nacre_0 + (__nacre_1 * 3))"));
    assert!(bash.contains("d=$(awk -v __nacre_0=\"$a\" -v __nacre_1=\"$b\""));
    assert!(bash.contains("(__nacre_0 - (__nacre_1 / 2))"));
    assert!(bash.contains("readonly escaped='a\"b'"));
    assert!(bash.contains("readonly quoted='a'\\''b'"));
    assert!(bash.contains("readonly noOp='a + b'"));
}

#[test]
fn compiles_all_comparison_operators() {
    let bash = compile_source(
        r#"
const ne = 1 != 2
const le = 1 <= 2
const gt = 2 > 1
const ge = 2 >= 1
"#,
    )
    .unwrap();

    assert!(bash.contains("readonly ne=$(awk "));
    assert!(bash.contains("(1 != 2)"));
    assert!(bash.contains("readonly le=$(awk "));
    assert!(bash.contains("(1 <= 2)"));
    assert!(bash.contains("readonly gt=$(awk "));
    assert!(bash.contains("(2 > 1)"));
    assert!(bash.contains("readonly ge=$(awk "));
    assert!(bash.contains("(2 >= 1)"));
}

#[test]
fn compiles_annotated_primitive_literals() {
    let bash = compile_source(
        r#"
const hex: Int = 0xFF
const bits = 0b1010
const pi: Float = 3.14
const unit: Unit = ()
const path: Path = "/tmp"
const shell = env.SHELL
const name = "Nacre"
fn greet(who: String, prefix: String = "Hello"): String {
return "${prefix}, ${who}"
}
const message = greet(name)
const custom = greet(name, "Hi")
const names: [String] = ["alice", "bob"]
const [firstUser, ...remainingUsers] = names
const label = if 1 < 2 { "positive" } else { "zero" }
const matched = match label { "positive" => "yes", _ => "no" }
let nums = [1, 2, 3]
nums = [4, 5]
const envs: Map[String, String] = { "PORT": "8080", "HOST": "localhost" }
let codes = { "ok": 200 }
codes = { "accepted": 202 }
const port = envs["PORT"]
const firstName = names[0]
const nameCount = names.len()
const pair: (String, Int) = ("localhost", 8080)
const hostName = pair._1
const portNumber = pair._2
const (destructuredHost, destructuredPort) = pair
const user: { name: String, age: Int } = { name: "Ada", age: 36 }
const userName = user.name
const userAge = user.age
let { age } = user
type Account = { id: Int, name: String }
const account: Account = { id: 1, name: "core" }
const accountName = account.name
newtype UserId = Int
const uid: UserId = UserId(42)
const rawUid: Int = uid.value
const greeting = "Hello, ${name}"
const rawGreeting = r"Hello, ${name}"
const hasGit = hasCommand("git")
let count = 5
count = count % 2
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
for person in names {
const copiedPerson = person
}
"#,
    )
    .unwrap();

    assert!(bash.contains("readonly hex=255"));
    assert!(bash.contains("readonly bits=10"));
    assert!(bash.contains("readonly pi=3.14"));
    assert!(bash.contains("readonly unit=''"));
    assert!(bash.contains("readonly path='/tmp'"));
    assert!(bash.contains("readonly shell=\"${SHELL}\""));
    assert!(bash.contains("greet() {\nlocal __nacre_local_greet_0_who=\"$1\""));
    assert!(bash.contains("readonly greet='greet'"));
    assert!(bash.contains("local __nacre_local_greet_1_prefix='Hello'"));
    assert!(bash.contains(
            "local __nacre_return_value\n__nacre_return_value=\"${__nacre_local_greet_1_prefix}, ${__nacre_local_greet_0_who}\"\nprintf '%s\\n' \"$__nacre_return_value\"\nreturn 0"
        ));
    assert!(bash.contains("readonly message=\"$(__nacre_call \"$greet\" \"$name\")\""));
    assert!(bash.contains("readonly custom=\"$(__nacre_call \"$greet\" \"$name\" 'Hi')\""));
    assert!(bash.contains("readonly firstUser=\"${names[0]}\""));
    assert!(bash.contains("readonly -a remainingUsers=(\"${names[@]:1}\")"));
    assert!(bash.contains(
            "readonly label=$(if awk 'BEGIN { exit (((1 < 2)) ? 0 : 1) }'; then printf '%s\\n' 'positive'; else printf '%s\\n' 'zero'; fi)"
        ));
    assert!(bash.contains("readonly matched=\"$(__nacre_match=\"$label\"; if case \"$__nacre_match\" in 'positive') true ;; *) false ;; esac; then printf '%s\\n' 'yes'; elif true; then printf '%s\\n' 'no'; fi)\""));
    assert!(bash.contains("readonly -a names=('alice' 'bob')"));
    assert!(bash.contains("nums=(1 2 3)"));
    assert!(bash.contains("nums=(4 5)"));
    assert!(bash.contains("declare -Ar envs=(['PORT']='8080' ['HOST']='localhost')"));
    assert!(bash.contains("declare -A codes=(['ok']=200)"));
    assert!(bash.contains("declare -A codes=(['accepted']=202)"));
    assert!(bash.contains("readonly port=\"${envs['PORT']}\""));
    assert!(bash.contains("readonly firstName=\"${names[0]}\""));
    assert!(bash.contains("readonly nameCount=\"${#names[@]}\""));
    assert!(bash.contains("readonly pair_1='localhost'"));
    assert!(bash.contains("readonly pair_2=8080"));
    assert!(bash.contains("readonly hostName=\"$pair_1\""));
    assert!(bash.contains("readonly portNumber=\"$pair_2\""));
    assert!(bash.contains("readonly destructuredHost=\"$pair_1\""));
    assert!(bash.contains("readonly destructuredPort=\"$pair_2\""));
    assert!(bash.contains("readonly user_name='Ada'"));
    assert!(bash.contains("readonly user_age=36"));
    assert!(bash.contains("readonly userName=\"$user_name\""));
    assert!(bash.contains("readonly userAge=\"$user_age\""));
    assert!(bash.contains("age=\"$user_age\""));
    assert!(bash.contains("readonly account_id=1"));
    assert!(bash.contains("readonly account_name='core'"));
    assert!(bash.contains("readonly accountName=\"$account_name\""));
    assert!(bash.contains("readonly uid=42"));
    assert!(bash.contains("readonly rawUid=\"$uid\""));
    assert!(bash.contains("readonly greeting=\"Hello, ${name}\""));
    assert!(bash.contains("readonly rawGreeting='Hello, ${name}'"));
    assert!(bash.contains(
        "readonly hasGit=$(command -v 'git' >/dev/null 2>&1 && printf true || printf false)"
    ));
    assert!(
        bash.contains("count=$(awk -v __nacre_0=\"$count\" 'BEGIN { print ((__nacre_0 % 2)) }')")
    );
    assert!(bash.contains(
        "if awk -v __nacre_0=\"$count\" 'BEGIN { exit (((__nacre_0 > 0)) ? 0 : 1) }'; then"
    ));
    assert!(bash.contains(
            "while awk -v __nacre_0=\"$count\" 'BEGIN { exit (((__nacre_0 > 0)) ? 0 : 1) }'; do\nif awk -v __nacre_0=\"$count\" 'BEGIN { exit (((__nacre_0 == 1)) ? 0 : 1) }'; then\nbreak\nfi\ncount=$(awk -v __nacre_0=\"$count\" 'BEGIN { print ((__nacre_0 - 1)) }')\ncontinue\ndone"
        ));
    assert!(bash.contains("for person in \"${names[@]}\"; do"));
}

#[test]
fn transpile_can_emit_unchecked_arithmetic_operands() {
    let program = parse(r#"let e = "1" + 2"#).unwrap();
    let bash = transpile(&program);

    assert!(bash.contains("e=$(awk 'BEGIN { print ((\"1\" + 2)) }')"));
}

#[test]
fn helper_emitters_cover_edge_case_shapes() {
    use std::collections::HashMap;

    let mut out = String::new();
    emit_expr_statement(&mut out, &Expr::Int(7));
    assert_eq!(out, "7\n");
    assert_eq!(sanitize_shell_ident("bad-name"), "bad_name");
    assert_eq!(sanitize_shell_ident(""), "_");
    assert!(!is_shell_name(""));

    let mut locals = HashMap::new();
    locals.insert("value".to_string(), "__local_value".to_string());
    assert_eq!(mangle_local_name("value", &locals), "__local_value");
    assert_eq!(mangle_local_name("global", &locals), "global");
    assert_eq!(
        mangle_call_name("value.method", &locals),
        "__local_value.method"
    );
    assert_eq!(mangle_call_name("value", &locals), "__local_value");
    assert_eq!(mangle_call_name("global", &locals), "global");
    assert_eq!(mangle_call_name("global.method", &locals), "global.method");
    assert_eq!(
        mangle_shell_interpolations("hello ${value} ${missing", &locals),
        "hello ${__local_value} ${missing"
    );
    let mut mangler = LocalMangler::new("outer");
    assert!(matches!(
        mangle_local_statement(
            &Statement::Expr(Expr::Ident("value".into())),
            &mut mangler,
            &mut locals.clone(),
        ),
        Statement::Expr(Expr::Ident(ref name)) if name == "__local_value"
    ));
    assert!(matches!(
        mangle_local_statement(&Statement::Break, &mut mangler, &mut locals.clone()),
        Statement::Break
    ));
    assert!(matches!(
        mangle_local_statement(
            &Statement::Const {
                name: "_".into(),
                annotation: None,
                expr: Expr::Ident("value".into()),
            },
            &mut mangler,
            &mut locals.clone(),
        ),
        Statement::Const { ref name, ref expr, .. }
            if name == "_" && *expr == Expr::Ident("__local_value".into())
    ));
    assert!(matches!(
        mangle_local_statement(
            &Statement::Function {
                name: "inner".into(),
                override_constructor: false,
                type_params: Vec::new(),
                params: Vec::new(),
                return_type: Type::Unit,
                body: Program::new(Vec::new(), Vec::new()),
            },
            &mut mangler,
            &mut locals.clone(),
        ),
        Statement::Function { ref name, .. } if name == "inner"
    ));

    let expr = Expr::Match {
        value: Box::new(Expr::Ident("value".into())),
        arms: vec![MatchArm {
            pattern: Some(Expr::Ident("value".into())),
            guard: None,
            expr: Expr::IfElse {
                condition: Box::new(Expr::Ident("value".into())),
                then_expr: Box::new(Expr::NewtypeCtor {
                    name: "UserId".into(),
                    value: Box::new(Expr::Value("value".into())),
                }),
                else_expr: Box::new(Expr::Len("value".into())),
            },
        }],
    };
    let mangled = mangle_local_expr(&expr, &locals);
    assert!(matches!(
        mangled,
        Expr::Match { ref value, .. } if **value == Expr::Ident("__local_value".into())
    ));

    out.clear();
    emit_record_value(
        &mut out,
        &[
            ("name".into(), Expr::String("Ada".into())),
            ("age".into(), Expr::Int(36)),
        ],
    );
    assert_eq!(out, "('Ada' 36)");
    out.clear();
    emit_tuple_value(&mut out, &[Expr::Int(1), Expr::RawString("two".into())]);
    assert_eq!(out, "(1 'two')");

    out.clear();
    emit_assignment(
        &mut out,
        "pair",
        &Expr::Tuple(vec![
            Expr::String("left".into()),
            Expr::String("right".into()),
        ]),
    );
    assert!(out.contains("pair_1='left'"));
    assert!(out.contains("pair_2='right'"));
    out.clear();
    emit_assignment(
        &mut out,
        "user",
        &Expr::Record(vec![("name".into(), Expr::String("Ada".into()))]),
    );
    assert_eq!(out, "user_name='Ada'\n");

    out.clear();
    emit_async_binding(&mut out, "job", "printf ok", true, false);
    assert!(out.contains("readonly job_out job_pid"));
    out.clear();
    emit_await_binding(&mut out, "result", "job", true, false);
    assert!(out.contains("readonly result"));
    out.clear();
    emit_assignment(&mut out, "job", &Expr::AsyncCommand("printf ok".into()));
    assert!(out.contains("job_pid=$!"));
    out.clear();
    emit_assignment(&mut out, "result", &Expr::Await("job".into()));
    assert!(out.contains("if wait \"$job_pid\""));
    out.clear();
    emit_binding(
        &mut out,
        "_",
        &Expr::Command {
            command: "printf hidden".into(),
            checked: true,
        },
        true,
        false,
    );
    assert_eq!(out, "printf hidden >/dev/null || exit $?\n");
    out.clear();
    emit_assignment(
        &mut out,
        "_",
        &Expr::Call {
            name: "value".into(),
            args: vec![Expr::String("x".into())],
        },
    );
    assert_eq!(out, "__nacre_call \"$value\" 'x' >/dev/null\n");
    out.clear();
    emit_for_iterable(
        &mut out,
        &Expr::Array(vec![Expr::String("a".into()), Expr::String("b".into())]),
    );
    assert_eq!(out, "'a' 'b'");
    out.clear();
    emit_for_iterable(&mut out, &Expr::String("single".into()));
    assert_eq!(out, "'single'");
    out.clear();
    emit_bound_expr(
        &mut out,
        &Expr::Command {
            command: "false".into(),
            checked: true,
        },
    );
    assert_eq!(out, "\"$(false)\" || exit $?\n");

    out.clear();
    emit_index_expr(&mut out, &Expr::Ident("index".into()));
    assert_eq!(out, "index");
    out.clear();
    emit_expr(&mut out, &Expr::AsyncCommand("printf ok".into()));
    assert_eq!(out, "'printf ok'");
    out.clear();
    emit_expr(&mut out, &Expr::Await("job".into()));
    assert_eq!(out, "\"$(cat \"$job_out\")\"");
    out.clear();
    emit_expr(&mut out, &Expr::Array(vec![Expr::Int(1), Expr::Int(2)]));
    assert_eq!(out, "(1 2)");
    out.clear();
    emit_expr(
        &mut out,
        &Expr::Map(vec![(Expr::Int(1), Expr::String("one".into()))]),
    );
    assert_eq!(out, "([1]='one')");
    out.clear();
    emit_expr(
        &mut out,
        &Expr::Record(vec![("name".into(), Expr::String("Ada".into()))]),
    );
    assert_eq!(out, "('Ada')");
    out.clear();
    emit_expr(&mut out, &Expr::Tuple(vec![Expr::Int(1), Expr::Int(2)]));
    assert_eq!(out, "(1 2)");
    out.clear();
    emit_expr(
        &mut out,
        &Expr::Binary {
            left: Box::new(Expr::Bool(true)),
            op: crate::BinaryOp::Eq,
            right: Box::new(Expr::Bool(false)),
        },
    );
    assert!(out.contains("awk"));
    out.clear();
    emit_array_element(
        &mut out,
        &Expr::EnvDefault {
            name: "HOME".into(),
            default: "/tmp".into(),
        },
    );
    assert_eq!(out, "\"${HOME:-/tmp}\"");
    out.clear();
    emit_array_element(&mut out, &Expr::Float("1.5".into()));
    assert_eq!(out, "1.5");
    out.clear();
    emit_array_element(&mut out, &Expr::Bool(true));
    assert_eq!(out, "true");
    out.clear();
    emit_array_element(&mut out, &Expr::Bool(false));
    assert_eq!(out, "false");
    out.clear();
    emit_array_element(&mut out, &Expr::Unit);
    assert_eq!(out, "''");
    out.clear();
    emit_array_element(
        &mut out,
        &Expr::Index {
            name: "xs".into(),
            index: Box::new(Expr::Int(0)),
        },
    );
    assert_eq!(out, "\"${xs[0]}\"");
    out.clear();
    emit_array_element(
        &mut out,
        &Expr::TupleField {
            name: "pair".into(),
            field: 1,
        },
    );
    assert_eq!(out, "\"$pair_1\"");
    out.clear();
    emit_array_element(
        &mut out,
        &Expr::Field {
            name: "user".into(),
            field: "name".into(),
        },
    );
    assert_eq!(out, "\"$user_name\"");
    out.clear();
    emit_array_element(
        &mut out,
        &Expr::Call {
            name: "make".into(),
            args: vec![Expr::String("x".into())],
        },
    );
    assert_eq!(out, "\"$(__nacre_call \"$make\" 'x')\"");
    out.clear();
    emit_array_element(
        &mut out,
        &Expr::NewtypeCtor {
            name: "UserId".into(),
            value: Box::new(Expr::Value("id".into())),
        },
    );
    assert_eq!(out, "\"$id\"");
    out.clear();
    emit_array_element(&mut out, &Expr::Len("xs".into()));
    assert_eq!(out, "\"${#xs[@]}\"");
    out.clear();
    emit_call_arg(&mut out, &Expr::Unit);
    assert_eq!(out, "''");
    out.clear();
    emit_call_arg(&mut out, &Expr::Float("2.5".into()));
    assert_eq!(out, "2.5");
    out.clear();
    emit_call_arg(&mut out, &Expr::Bool(true));
    assert_eq!(out, "'true'");
    out.clear();
    emit_call_arg(&mut out, &Expr::Bool(false));
    assert_eq!(out, "'false'");
    out.clear();
    emit_call_arg(&mut out, &Expr::RawString("raw".into()));
    assert_eq!(out, "'raw'");
    out.clear();
    emit_call_arg(
        &mut out,
        &Expr::Index {
            name: "xs".into(),
            index: Box::new(Expr::Int(0)),
        },
    );
    assert_eq!(out, "\"${xs[0]}\"");
    out.clear();
    emit_call_arg(
        &mut out,
        &Expr::TupleField {
            name: "pair".into(),
            field: 2,
        },
    );
    assert_eq!(out, "\"$pair_2\"");
    out.clear();
    emit_call_arg(
        &mut out,
        &Expr::Field {
            name: "user".into(),
            field: "name".into(),
        },
    );
    assert_eq!(out, "\"$user_name\"");
    out.clear();
    emit_call_arg(
        &mut out,
        &Expr::Call {
            name: "make".into(),
            args: vec![Expr::String("x".into())],
        },
    );
    assert_eq!(out, "\"$(__nacre_call \"$make\" 'x')\"");
    out.clear();
    emit_call_arg(&mut out, &Expr::Value("id".into()));
    assert_eq!(out, "\"$id\"");
    out.clear();
    emit_call_arg(
        &mut out,
        &Expr::NewtypeCtor {
            name: "UserId".into(),
            value: Box::new(Expr::Len("xs".into())),
        },
    );
    assert_eq!(out, "\"${#xs[@]}\"");
    out.clear();
    emit_call_arg(
        &mut out,
        &Expr::Binary {
            left: Box::new(Expr::Int(1)),
            op: crate::BinaryOp::Add,
            right: Box::new(Expr::Int(2)),
        },
    );
    assert!(out.contains("awk"));
    out.clear();
    emit_match_pattern(
        &mut out,
        &Expr::NewtypeCtor {
            name: "Flag".into(),
            value: Box::new(Expr::Bool(false)),
        },
    );
    assert_eq!(out, "'false'");
    out.clear();
    emit_match_pattern(&mut out, &Expr::Float("1.5".into()));
    assert_eq!(out, "1.5");
    out.clear();
    emit_match_pattern(&mut out, &Expr::Bool(true));
    assert_eq!(out, "'true'");
    out.clear();
    emit_match_pattern(&mut out, &Expr::Int(7));
    assert_eq!(out, "7");
    out.clear();
    emit_match_pattern(
        &mut out,
        &Expr::Index {
            name: "xs".into(),
            index: Box::new(Expr::Int(0)),
        },
    );
    assert_eq!(out, "\"${xs[0]}\"");
    out.clear();
    emit_awk_expr(
        &mut out,
        &Expr::Call {
            name: "value".into(),
            args: Vec::new(),
        },
        &mut Vec::new(),
    );
    assert_eq!(out, "__nacre_0");
    out.clear();
    emit_awk_expr(&mut out, &Expr::Unit, &mut Vec::new());
    assert_eq!(out, "\"\"");
    out.clear();
    emit_awk_expr(
        &mut out,
        &Expr::NewtypeCtor {
            name: "UserId".into(),
            value: Box::new(Expr::Float("1.5".into())),
        },
        &mut Vec::new(),
    );
    assert_eq!(out, "1.5");
    out.clear();
    emit_map_key(&mut out, &Expr::Int(7));
    assert_eq!(out, "7");
    out.clear();
    emit_map_key(&mut out, &Expr::Ident("key".into()));
    assert_eq!(out, "\"$key\"");
    out.clear();
    emit_map_key(
        &mut out,
        &Expr::Binary {
            left: Box::new(Expr::Int(1)),
            op: crate::BinaryOp::Add,
            right: Box::new(Expr::Int(2)),
        },
    );
    assert!(out.contains("awk"));
    out.clear();
    emit_interpolated_string(&mut out, "${value}\"\\`");
    assert_eq!(out, "\"${value}\\\"\\\\\\`\"");
    out.clear();
    emit_awk_string(&mut out, "a\"\\\n\r\t");
    assert_eq!(out, r#""a\"\\\n\r\t""#);
}
